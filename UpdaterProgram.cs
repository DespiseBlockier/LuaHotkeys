using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace LuaHotkeys
{
    internal static class UpdaterProgram
    {
        private const string AppName = "LuaHotkeys.exe";
        private const string UpdaterName = "LuaHotkeysUpdater.exe";
        private const string Title = "LuaHotkeys Updater";

        [STAThread]
        private static int Main(string[] args)
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            try
            {
                Guid run;
                int processId;
                if (args.Length == 3 && args[0] == "--cleanup" && Guid.TryParseExact(args[1], "N", out run) &&
                    Int32.TryParse(args[2], NumberStyles.None, CultureInfo.InvariantCulture, out processId))
                {
                    if (WaitForExit(processId))
                        CleanTemporaryCopy(run);
                    return 0;
                }
                if (args.Length == 4 && args[0] == "--apply" && Guid.TryParseExact(args[1], "N", out run) &&
                    Int32.TryParse(args[2], NumberStyles.None, CultureInfo.InvariantCulture, out processId) &&
                    SamePath(Path.GetDirectoryName(Application.ExecutablePath), TempFolder(run)))
                {
                    string folder = UpdatePaths.Normalize(Encoding.UTF8.GetString(Convert.FromBase64String(args[3])));
                    return RunTemporaryCopy(run, processId, folder);
                }
                if (args.Length != 0)
                    throw new ArgumentException("Open LuaHotkeysUpdater.exe to check for an update.");

                StartTemporaryCopy(UpdatePaths.Normalize(Path.GetDirectoryName(Application.ExecutablePath)));
                return 0;
            }
            catch (Exception error)
            {
                MessageBox.Show(error.Message, Title, MessageBoxButtons.OK, MessageBoxIcon.Error);
                return 1;
            }
        }

        private static void StartTemporaryCopy(string folder)
        {
            if (!File.Exists(Path.Combine(folder, AppName)))
                throw new FileNotFoundException("Keep LuaHotkeysUpdater.exe beside LuaHotkeys.exe.");

            Guid run = Guid.NewGuid();
            string temporary = TempFolder(run);
            Directory.CreateDirectory(temporary);
            try
            {
                string helper = Path.Combine(temporary, UpdaterName);
                File.Copy(Application.ExecutablePath, helper);
                string encodedFolder = Convert.ToBase64String(Encoding.UTF8.GetBytes(folder));
                Launch(helper, "--apply " + run.ToString("N") + " " + CurrentProcessId() + " " + encodedFolder);
            }
            catch
            {
                CleanTemporaryCopy(run);
                throw;
            }
        }

        private static int RunTemporaryCopy(Guid run, int launcherId, string folder)
        {
            try
            {
                if (!WaitForExit(launcherId))
                    throw new InvalidOperationException("Close the other updater and try again.");

                using (var window = new UpdateWindow((progress, retry) => RunUpdate(folder, TempFolder(run), progress, retry)))
                {
                    Application.Run(window);
                    return window.Result;
                }
            }
            finally
            {
                try
                {
                    string updater = Path.Combine(folder, UpdaterName);
                    Launch(updater, "--cleanup " + run.ToString("N") + " " + CurrentProcessId());
                }
                catch (Win32Exception) { }
            }
        }

        internal static string RunUpdate(string folder, string temporary, Action<string> progress, Func<bool> retryClose)
        {
            folder = UpdatePaths.Normalize(folder);
            using (var mutex = new Mutex(false, UpdatePaths.MutexName(folder)))
            {
                if (!UpdatePaths.TryAcquire(mutex))
                    return "LuaHotkeys is already being updated in this folder.";
                try
                {
                    bool installed = UpdateInstaller.IsInstalled(folder);
                    if (!installed && Directory.Exists(PortableUpdate.StagingFolder(folder)))
                    {
                        CloseRunningApp(folder, progress, retryClose);
                        PortableUpdate.Recover(folder);
                    }

                    string executable = Path.Combine(folder, AppName);
                    Version current = UpdateInstaller.FileVersion(executable);
                    progress("Checking for updates…");
                    GitHubRelease release = GitHubRelease.Check(current, installed);
                    if (release.Asset == null)
                        return "LuaHotkeys " + current.ToString(3) + " is up to date.";

                    string package = Path.Combine(temporary, release.Asset.Name);
                    try
                    {
                        progress("Downloading LuaHotkeys " + release.Version.ToString(3) + "…");
                        release.Download(package);
                        CloseRunningApp(folder, progress, retryClose);
                        progress("Installing the update…");
                        if (installed)
                        {
                            if (UpdateInstaller.Apply(folder, package, release.Version))
                                return "Restart Windows to finish updating LuaHotkeys.";
                        }
                        else
                        {
                            PortableUpdate.Apply(folder, package, release.Version);
                        }
                        return "LuaHotkeys was updated to " + release.Version.ToString(3) + ".";
                    }
                    finally
                    {
                        TryDelete(package);
                    }
                }
                finally
                {
                    mutex.ReleaseMutex();
                }
            }
        }

        private static void CloseRunningApp(string folder, Action<string> progress, Func<bool> retryClose)
        {
            string executable = Path.Combine(folder, AppName);
            List<int> processes = AppProcesses(executable);
            if (processes.Count == 0)
                return;
            progress("Waiting for LuaHotkeys to close…");
            foreach (int id in processes)
            {
                EventWaitHandle close;
                if (EventWaitHandle.TryOpenExisting(UpdatePaths.CloseEventName(id), out close))
                    using (close) { close.Set(); }
            }
            var wait = Stopwatch.StartNew();
            while (AppProcesses(executable).Count != 0)
            {
                if (wait.ElapsedMilliseconds >= 5000)
                {
                    if (!retryClose())
                        throw new OperationCanceledException();
                    wait.Restart();
                }
                Thread.Sleep(200);
            }
        }

        private static List<int> AppProcesses(string executable)
        {
            var matches = new List<int>();
            foreach (var process in Process.GetProcessesByName("LuaHotkeys"))
            {
                using (process)
                {
                    try
                    {
                        if (SamePath(process.MainModule.FileName, executable))
                            matches.Add(process.Id);
                    }
                    catch (Win32Exception) { }
                    catch (InvalidOperationException) { }
                }
            }
            return matches;
        }

        private static bool SamePath(string left, string right)
        {
            return String.Equals(Path.GetFullPath(left), Path.GetFullPath(right), StringComparison.OrdinalIgnoreCase);
        }

        private static int CurrentProcessId()
        {
            using (var process = Process.GetCurrentProcess())
                return process.Id;
        }

        private static void Launch(string executable, string arguments)
        {
            var options = new ProcessStartInfo(executable, arguments)
            {
                WorkingDirectory = Path.GetDirectoryName(executable),
                UseShellExecute = false,
                CreateNoWindow = true
            };
            using (Process.Start(options)) { }
        }

        private static bool WaitForExit(int id)
        {
            if (id <= 0 || id == CurrentProcessId())
                return false;
            try
            {
                using (var process = Process.GetProcessById(id))
                    return process.WaitForExit(15000);
            }
            catch (ArgumentException) { return true; }
        }

        private static string TempFolder(Guid run)
        {
            return Path.Combine(Path.GetTempPath(), "LuaHotkeys-Update-" + run.ToString("N"));
        }

        private static void CleanTemporaryCopy(Guid run)
        {
            string temporary = TempFolder(run);
            TryDelete(Path.Combine(temporary, UpdaterName));
            try { Directory.Delete(temporary); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }

        private static void TryDelete(string file)
        {
            try { File.Delete(file); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }

        private sealed class UpdateWindow : Form
        {
            internal int Result { get; private set; }
            private readonly Label status = new Label();
            private readonly ProgressBar bar = new ProgressBar();

            internal UpdateWindow(Func<Action<string>, Func<bool>, string> run)
            {
                Text = Title;
                ClientSize = new Size(440, 104);
                Font = new Font("Segoe UI", 10F);
                FormBorderStyle = FormBorderStyle.FixedDialog;
                ControlBox = false;
                StartPosition = FormStartPosition.CenterScreen;
                status.SetBounds(18, 13, 404, 34);
                status.TextAlign = ContentAlignment.MiddleLeft;
                status.Text = "Starting updater…";
                bar.SetBounds(18, 54, 404, 16);
                bar.Style = ProgressBarStyle.Marquee;
                Controls.AddRange(new Control[] { status, bar });
                Shown += async (sender, args) =>
                {
                    var report = (IProgress<string>)new Progress<string>(message => status.Text = message);
                    try
                    {
                        string message = await Task.Run(() => run(report.Report, AskToClose));
                        MessageBox.Show(this, message, Title, MessageBoxButtons.OK, MessageBoxIcon.Information);
                    }
                    catch (OperationCanceledException)
                    {
                        Result = 2;
                    }
                    catch (Exception error)
                    {
                        Result = 1;
                        MessageBox.Show(this, error.Message, Title, MessageBoxButtons.OK, MessageBoxIcon.Error);
                    }
                    Close();
                };
            }

            private bool AskToClose()
            {
                return (bool)Invoke(new Func<bool>(() => MessageBox.Show(this,
                    "Save your edits and exit LuaHotkeys from the system tray, then select Retry.",
                    Title, MessageBoxButtons.RetryCancel, MessageBoxIcon.Information) == DialogResult.Retry));
            }
        }
    }
}
