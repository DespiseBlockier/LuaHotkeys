using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace LuaHotkeys
{
    internal static class Program
    {
        [STAThread]
        private static int Main(string[] arguments)
        {
            WindowsInput.SetProcessDPIAware();
            if (arguments.Length == 1 && arguments[0] == "--worker")
                return LuaWorker.Run();

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            string script = null;
            bool runInTray = false;
            if (arguments.Length == 1 && !arguments[0].StartsWith("--", StringComparison.Ordinal))
            {
                script = arguments[0];
                runInTray = String.Equals(Path.GetExtension(script), FileAssociation.Extension, StringComparison.OrdinalIgnoreCase);
            }
            else if (arguments.Length == 2 && (arguments[0] == "--edit" || arguments[0] == "--run"))
            {
                script = arguments[1];
                runInTray = arguments[0] == "--run";
            }
            else if (arguments.Length != 0)
            {
                MessageBox.Show("Use LuaHotkeys.exe --run <file> or LuaHotkeys.exe --edit <file>.", "LuaHotkeys");
                return 1;
            }

            using (var form = new MainForm())
            {
                if (script != null && !form.LoadScript(script))
                    return 1;
                if (runInTray)
                    form.RunInTray();
                else
                    Application.Run(form);
            }
            return 0;
        }
    }

    internal sealed class MainForm : Form
    {
        private readonly Color ink = Color.FromArgb(26, 36, 54);
        private readonly Color muted = Color.FromArgb(75, 91, 115);
        private readonly Color accent = Color.FromArgb(34, 84, 172);
        private readonly LuaEditor editor = new LuaEditor();
        private readonly RichTextBox activity = new RichTextBox();
        private readonly Label fileLabel = new Label();
        private readonly Label status = new Label();
        private readonly Button runButton;
        private readonly Button stopButton;
        private readonly NotifyIcon tray;
        private readonly ToolStripMenuItem trayStop;
        private Process worker;
        private string path;
        private string savedText;

        private bool launchedInTray;

        internal MainForm()
        {
            Text = "LuaHotkeys";
            Font = new Font("Segoe UI", 10F);
            BackColor = Color.FromArgb(243, 246, 251);
            ForeColor = ink;
            MinimumSize = new Size(700, 475);
            ClientSize = new Size(860, 600);
            StartPosition = FormStartPosition.CenterScreen;
            AutoScaleMode = AutoScaleMode.Dpi;
            Icon = SystemIcons.Application;

            var layout = new TableLayoutPanel
            {
                ColumnCount = 1, RowCount = 7, Dock = DockStyle.Fill, Padding = new Padding(20, 14, 20, 10)
            };
            layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 47));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 50));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 27));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 72));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 32));
            layout.RowStyles.Add(new RowStyle(SizeType.Percent, 28));
            layout.RowStyles.Add(new RowStyle(SizeType.Absolute, 30));
            Controls.Add(layout);

            layout.Controls.Add(new Label
            {
                Text = "LuaHotkeys", Font = new Font("Segoe UI", 23F, FontStyle.Bold),
                Dock = DockStyle.Fill, Margin = Padding.Empty, TextAlign = ContentAlignment.MiddleLeft
            }, 0, 0);

            var commands = new FlowLayoutPanel
            {
                Dock = DockStyle.Fill, Margin = Padding.Empty, WrapContents = false, Padding = new Padding(0, 7, 0, 0)
            };
            runButton = Command("Run", RunScript, true);
            stopButton = Command("Stop", StopScript, false);
            commands.Controls.AddRange(new Control[]
            {
                runButton, stopButton, Command("Open…", OpenScript, false),
                Command("Save", () => SaveScript(), false), Command("Hide to tray", Hide, false),
                Command("Associate files", AssociateFiles, false)
            });
            layout.Controls.Add(commands, 0, 1);

            fileLabel.Dock = DockStyle.Fill;
            fileLabel.AutoEllipsis = true;
            fileLabel.TextAlign = ContentAlignment.MiddleLeft;
            fileLabel.ForeColor = muted;
            layout.Controls.Add(fileLabel, 0, 2);

            ConfigureTextBox(editor, false);
            editor.AccessibleName = "Lua script";
            editor.AcceptsTab = true;
            editor.WordWrap = false;
            editor.BackColor = Color.FromArgb(20, 30, 48);
            editor.ForeColor = Color.FromArgb(234, 240, 252);
            editor.Font = new Font("Consolas", 11F);
            editor.TextChanged += (sender, args) => UpdateFileLabel();
            layout.Controls.Add(editor, 0, 3);

            layout.Controls.Add(new Label
            {
                Text = "Activity", Font = new Font("Segoe UI", 10F, FontStyle.Bold),
                Dock = DockStyle.Fill, TextAlign = ContentAlignment.MiddleLeft
            }, 0, 4);
            ConfigureTextBox(activity, true);
            activity.AccessibleName = "Activity log";
            activity.BackColor = Color.White;
            activity.ForeColor = ink;
            activity.Font = new Font("Consolas", 9.5F);
            layout.Controls.Add(activity, 0, 5);

            status.Dock = DockStyle.Fill;
            status.AccessibleName = "Script status";
            status.TextAlign = ContentAlignment.MiddleLeft;
            status.ForeColor = muted;
            layout.Controls.Add(status, 0, 6);

            var menu = new ContextMenuStrip();
            menu.Items.Add("Edit script", null, (sender, args) => ShowEditor());
            trayStop = new ToolStripMenuItem("Stop script", null, (sender, args) => StopScript());
            menu.Items.Add(trayStop);
            menu.Items.Add(new ToolStripSeparator());
            menu.Items.Add("Exit", null, (sender, args) => { ShowEditor(); Close(); });
            tray = new NotifyIcon { Icon = Icon, Text = "LuaHotkeys", ContextMenuStrip = menu, Visible = true };
            tray.DoubleClick += (sender, args) => ShowEditor();
            SetStatus("Stopped", false);

            using (var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("example.lua"))
            using (var reader = new StreamReader(stream))
                editor.Text = reader.ReadToEnd();
            savedText = editor.Text;
            UpdateFileLabel();
            Append("Run the example, then press Ctrl+Alt+J in a text editor.");

        }

        internal void RunInTray()
        {
            launchedInTray = true;
            using (var context = new ApplicationContext())
            {
                FormClosed += (sender, args) => context.ExitThread();
                // The hidden form needs a handle for updates from the script process.
                CreateHandle();
                RunScript();
                Application.Run(context);
            }
        }

        private void AssociateFiles()
        {
            try
            {
                FileAssociation.Register(Application.ExecutablePath);
                Append("Double-click a .luahotkeys file to run it, or right-click and choose Edit in LuaHotkeys.");
            }
            catch (Exception error)
            {
                MessageBox.Show(this, error.Message, "Could not associate files", MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }

        private Button Command(string caption, Action action, bool primary)
        {
            var button = new Button
            {
                Text = caption, AutoSize = true, MinimumSize = new Size(74, 32), Height = 32,
                BackColor = primary ? accent : Color.White, ForeColor = primary ? Color.White : ink,
                Padding = new Padding(10, 0, 10, 0), Margin = new Padding(0, 0, 8, 0),
                FlatStyle = FlatStyle.Flat, Cursor = Cursors.Hand, UseVisualStyleBackColor = false
            };
            button.FlatAppearance.BorderColor = primary ? accent : Color.FromArgb(200, 210, 224);
            button.Click += (sender, args) => action();
            return button;
        }

        private static void ConfigureTextBox(RichTextBox control, bool readOnly)
        {
            control.Dock = DockStyle.Fill;
            control.Margin = Padding.Empty;
            control.BorderStyle = BorderStyle.FixedSingle;
            control.DetectUrls = false;
            control.ReadOnly = readOnly;
            control.ScrollBars = RichTextBoxScrollBars.Both;
        }

        private void RunScript()
        {
            if (!StopScriptCore())
                return;

            var start = new ProcessStartInfo(Application.ExecutablePath, "--worker")
            {
                UseShellExecute = false, CreateNoWindow = true, RedirectStandardInput = true,
                RedirectStandardOutput = true, WorkingDirectory = path == null
                    ? AppDomain.CurrentDomain.BaseDirectory : Path.GetDirectoryName(path)
            };
            Process process;
            try { process = Process.Start(start); }
            catch (Exception error)
            {
                Append(error.Message);
                if (launchedInTray)
                    ShowEditor();
                return;
            }

            worker = process;
            string name = path == null ? "Untitled.luahotkeys" : Path.GetFileName(path);
            string source = editor.Text;
            SetStatus("Starting…", true);
            Append("Running " + name);
            ThreadPool.QueueUserWorkItem(ignored => ReadWorker(process, name, source));
        }

        private void ReadWorker(Process process, string name, string source)
        {
            bool completed = false;
            try
            {
                using (var input = process.StandardInput)
                using (var output = process.StandardOutput)
                {
                    input.WriteLine(Wire.Encode(name));
                    input.WriteLine(Wire.Encode(source));
                    input.Flush();
                    string line;
                    while ((line = output.ReadLine()) != null)
                    {
                        string message = line;
                        UpdateFromWorker(process, () => HandleWorkerMessage(message));
                    }
                    process.WaitForExit();
                    int code = process.ExitCode;
                    completed = code == 0;
                    if (!completed)
                        UpdateFromWorker(process, () => Append("Script process exited with code " + code + "."));
                }
            }
            catch (Exception error)
            {
                UpdateFromWorker(process, () => Append(error.Message));
            }
            finally
            {
                UpdateFromWorker(process, () =>
                {
                    worker = null;
                    SetStatus("Stopped", false);
                    if (launchedInTray && !Visible)
                    {
                        if (completed)
                            Close();
                        else
                            ShowEditor();
                    }
                });
                process.Dispose();
            }
        }

        private void UpdateFromWorker(Process process, Action update)
        {
            try
            {
                Invoke(new MethodInvoker(() =>
                {
                    if (ReferenceEquals(worker, process))
                        update();
                }));
            }
            catch (InvalidOperationException) { }
        }

        private void HandleWorkerMessage(string message)
        {
            if (message.Length < 2 || message[1] != ' ')
            {
                Append(message);
                return;
            }
            string text;
            try { text = Wire.Decode(message.Substring(2)); }
            catch (FormatException) { Append(message); return; }
            switch (message[0])
            {
                case 'R':
                    int count;
                    if (Int32.TryParse(text, out count))
                    {
                        if (count == 0)
                            Append("Finished. No global hotkeys were declared.");
                        else
                            SetStatus("Running  •  " + count + (count == 1 ? " hotkey" : " hotkeys"), true);
                    }
                    break;
                case 'L': Append(text); break;
                case 'E': Append("Error  " + text); break;
                default: Append(message); break;
            }
        }

        private void StopScript()
        {
            if (worker != null && StopScriptCore())
                Append("Stopped.");
        }

        private bool StopScriptCore()
        {
            Process process = worker;
            if (process == null)
                return true;
            try
            {
                if (!process.HasExited)
                {
                    process.Kill();
                    if (!process.WaitForExit(1500))
                    {
                        Append("Waiting for the script process to stop.");
                        return false;
                    }
                }
            }
            catch (InvalidOperationException) { }
            catch (System.ComponentModel.Win32Exception error)
            {
                Append(error.Message);
                return false;
            }
            worker = null;
            SetStatus("Stopped", false);
            return true;
        }

        private void SetStatus(string caption, bool active)
        {
            status.Text = caption;
            stopButton.Enabled = active;
            trayStop.Enabled = active;
            runButton.Text = active ? "Restart" : "Run";
        }

        private void Append(string text)
        {
            const int maxLength = 48000;
            if (activity.TextLength > maxLength)
            {
                activity.Select(0, activity.TextLength - maxLength / 2);
                activity.SelectedText = String.Empty;
            }
            activity.AppendText(DateTime.Now.ToString("HH:mm:ss") + "  " + text.TrimEnd() + Environment.NewLine);
            activity.SelectionStart = activity.TextLength;
            activity.ScrollToCaret();
        }

        private void OpenScript()
        {
            if (!ConfirmDiscard())
                return;
            using (var dialog = new OpenFileDialog { Filter = "LuaHotkeys and Lua scripts (*.luahotkeys;*.lua)|*.luahotkeys;*.lua|All files (*.*)|*.*" })
                if (dialog.ShowDialog(this) == DialogResult.OK)
                    LoadScript(dialog.FileName);
        }

        internal bool LoadScript(string name)
        {
            try
            {
                string fullName = Path.GetFullPath(name);
                string script = File.ReadAllText(fullName, new UTF8Encoding(false, true));
                if (!StopScriptCore())
                    return false;
                path = fullName;
                editor.Text = script;
                savedText = editor.Text;
                UpdateFileLabel();
                return true;
            }
            catch (Exception error)
            {
                MessageBox.Show(this, error.Message, "Could not open script", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return false;
            }
        }

        private bool SaveScript()
        {
            string destination = path;
            if (destination == null)
            {
                using (var dialog = new SaveFileDialog
                {
                    Filter = "LuaHotkeys script (*.luahotkeys)|*.luahotkeys|Lua script (*.lua)|*.lua",
                    DefaultExt = "luahotkeys", FileName = "hotkeys.luahotkeys"
                })
                {
                    if (dialog.ShowDialog(this) != DialogResult.OK)
                        return false;
                    destination = dialog.FileName;
                }
            }
            try
            {
                File.WriteAllText(destination, editor.Text, new UTF8Encoding(false));
                path = destination;
                savedText = editor.Text;
                UpdateFileLabel();
                Append("Saved " + Path.GetFileName(path));
                return true;
            }
            catch (Exception error)
            {
                MessageBox.Show(this, error.Message, "Could not save script", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return false;
            }
        }

        private bool ConfirmDiscard()
        {
            if (editor.Text == savedText)
                return true;
            DialogResult answer = MessageBox.Show(this, "Save your changes to the Lua script?",
                "LuaHotkeys", MessageBoxButtons.YesNoCancel, MessageBoxIcon.Question);
            return answer == DialogResult.No || (answer == DialogResult.Yes && SaveScript());
        }

        private void UpdateFileLabel()
        {
            string name = path == null ? "Untitled.luahotkeys" : Path.GetFileName(path);
            fileLabel.Text = name + (savedText != null && editor.Text != savedText ? "  •  Unsaved" : String.Empty);
            if (tray != null)
            {
                string tooltip = "LuaHotkeys · " + name;
                tray.Text = tooltip.Length > 63 ? tooltip.Substring(0, 60) + "…" : tooltip;
            }
        }

        private void ShowEditor()
        {
            Show();
            if (WindowState == FormWindowState.Minimized)
                WindowState = FormWindowState.Normal;
            Activate();
        }

        protected override bool ProcessCmdKey(ref Message message, Keys keyData)
        {
            switch (keyData)
            {
                case Keys.F5: RunScript(); return true;
                case Keys.Shift | Keys.F5: StopScript(); return true;
                case Keys.Control | Keys.S: SaveScript(); return true;
                case Keys.Control | Keys.O: OpenScript(); return true;
                default: return base.ProcessCmdKey(ref message, keyData);
            }
        }

        protected override void OnFormClosing(FormClosingEventArgs e)
        {
            if (e.CloseReason == CloseReason.UserClosing && !ConfirmDiscard())
                e.Cancel = true;
            if (!e.Cancel)
                StopScriptCore();
            base.OnFormClosing(e);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing && tray != null && !IsDisposed)
            {
                tray.Visible = false;
                tray.ContextMenuStrip.Dispose();
                tray.Dispose();
            }
            base.Dispose(disposing);
        }
    }
}
