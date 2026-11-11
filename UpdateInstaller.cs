using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32;

namespace LuaHotkeys
{
    internal static class UpdateInstaller
    {
        private const string UpgradeCode = "{54B8D0E7-0923-46B0-8694-7F3280300F48}";

        internal static bool IsInstalled(string folder)
        {
            using (var key = Registry.CurrentUser.OpenSubKey(@"Software\LuaHotkeys\Installer"))
            {
                string location = key == null ? null : key.GetValue("InstallFolder") as string;
                return !String.IsNullOrWhiteSpace(location) &&
                    String.Equals(UpdatePaths.Normalize(location), folder, StringComparison.OrdinalIgnoreCase);
            }
        }

        internal static Version FileVersion(string executable)
        {
            var version = FileVersionInfo.GetVersionInfo(executable);
            return new Version(version.FileMajorPart, version.FileMinorPart, version.FileBuildPart);
        }

        internal static bool Apply(string folder, string package, Version version)
        {
            if (!String.Equals(ReadProperty(package, "UpgradeCode"), UpgradeCode, StringComparison.OrdinalIgnoreCase) ||
                ReadProperty(package, "ProductVersion") != version.ToString(3))
                throw new InvalidDataException("The installer does not match this LuaHotkeys release.");

            string installer = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "msiexec.exe");
            var options = new ProcessStartInfo(installer, "/i \"" + package + "\" /quiet /norestart")
            {
                UseShellExecute = false,
                CreateNoWindow = true,
                WorkingDirectory = Path.GetDirectoryName(package)
            };
            using (var process = Process.Start(options))
            {
                process.WaitForExit();
                if (process.ExitCode == 3010)
                    return true;
                if (process.ExitCode != 0)
                    throw new InvalidOperationException("Windows Installer returned error " + process.ExitCode + ".");
            }
            if (FileVersion(Path.Combine(folder, "LuaHotkeys.exe")) != version || !IsInstalled(folder))
                throw new IOException("Windows Installer did not update LuaHotkeys in the expected folder.");
            return false;
        }

        private static string ReadProperty(string package, string property)
        {
            uint database;
            Check(MsiOpenDatabase(package, IntPtr.Zero, out database));
            try
            {
                uint view;
                Check(MsiDatabaseOpenView(database, "SELECT \u0060Value\u0060 FROM \u0060Property\u0060 WHERE \u0060Property\u0060='" + property + "'", out view));
                try
                {
                    Check(MsiViewExecute(view, 0));
                    uint record;
                    Check(MsiViewFetch(view, out record));
                    try
                    {
                        var value = new StringBuilder(128);
                        uint length = (uint)value.Capacity;
                        Check(MsiRecordGetString(record, 1, value, ref length));
                        return value.ToString();
                    }
                    finally { MsiCloseHandle(record); }
                }
                finally { MsiCloseHandle(view); }
            }
            finally { MsiCloseHandle(database); }
        }

        private static void Check(uint result)
        {
            if (result != 0)
                throw new Win32Exception((int)result, "The update installer could not be validated.");
        }

        [DllImport("msi.dll", CharSet = CharSet.Unicode)]
        private static extern uint MsiOpenDatabase(string path, IntPtr persist, out uint database);
        [DllImport("msi.dll", CharSet = CharSet.Unicode)]
        private static extern uint MsiDatabaseOpenView(uint database, string query, out uint view);
        [DllImport("msi.dll")]
        private static extern uint MsiViewExecute(uint view, uint record);
        [DllImport("msi.dll")]
        private static extern uint MsiViewFetch(uint view, out uint record);
        [DllImport("msi.dll", CharSet = CharSet.Unicode)]
        private static extern uint MsiRecordGetString(uint record, uint field, StringBuilder value, ref uint length);
        [DllImport("msi.dll")]
        private static extern uint MsiCloseHandle(uint handle);
    }
}
