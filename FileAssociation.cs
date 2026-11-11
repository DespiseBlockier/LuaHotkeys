using System;
using System.Runtime.InteropServices;
using Microsoft.Win32;

namespace LuaHotkeys
{
    internal static class FileAssociation
    {
        internal const string Extension = ".lhk";
        internal const string LegacyExtension = ".luahotkeys";
        private const string ProgramId = "LuaHotkeys.Script";

        internal static bool IsScriptExtension(string extension)
        {
            return String.Equals(extension, Extension, StringComparison.OrdinalIgnoreCase) ||
                   String.Equals(extension, LegacyExtension, StringComparison.OrdinalIgnoreCase);
        }

        internal static void Register(string executable)
        {
            using (RegistryKey classes = Registry.CurrentUser.CreateSubKey(@"Software\Classes"))
            {
                WriteDefault(classes, ProgramId, "LuaHotkeys Script");
                WriteDefault(classes, ProgramId + @"\DefaultIcon", "\"" + executable + "\",0");
                WriteDefault(classes, ProgramId + @"\shell", "open");
                RegisterVerb(classes, executable, ProgramId + @"\shell\open", "Run with LuaHotkeys", "--run");
                RegisterVerb(classes, executable, ProgramId + @"\shell\edit", "Edit in LuaHotkeys", "--edit");


                const string app = @"Applications\LuaHotkeys.exe";
                using (RegistryKey application = classes.CreateSubKey(app))
                    application.SetValue("FriendlyAppName", "LuaHotkeys");

                WriteDefault(classes, app + @"\shell\open\command", "\"" + executable + "\" \"%1\"");
                RegisterVerb(classes, executable, app + @"\shell\edit", "Edit in LuaHotkeys", "--edit");
                using (RegistryKey types = classes.CreateSubKey(app + @"\SupportedTypes"))
                {
                    foreach (string extension in new[] { Extension, LegacyExtension })
                    {
                        using (RegistryKey openWith = classes.CreateSubKey(extension + @"\OpenWithProgids"))
                            openWith.SetValue(ProgramId, new byte[0], RegistryValueKind.None);
                        WriteDefault(classes, extension, ProgramId);
                        types.SetValue(extension, String.Empty);
                        RegisterVerb(classes, executable, @"SystemFileAssociations\" + extension + @"\shell\edit",
                                     "Edit in LuaHotkeys", "--edit");
                    }
                }
                using (RegistryKey shellNew = classes.CreateSubKey(Extension + @"\ShellNew"))
                    shellNew.SetValue("NullFile", String.Empty);
            }
            Refresh();
        }

        internal static void Refresh()
        {
            const int associationChanged = 0x08000000;
            SHChangeNotify(associationChanged, 0, IntPtr.Zero, IntPtr.Zero);
        }

        private static void RegisterVerb(RegistryKey classes, string executable, string key, string label, string option)
        {
            WriteDefault(classes, key, label);
            WriteDefault(classes, key + @"\command", "\"" + executable + "\" " + option + " \"%1\"");
        }

        private static void WriteDefault(RegistryKey root, string key, string value)
        {
            using (RegistryKey subkey = root.CreateSubKey(key))
                subkey.SetValue(String.Empty, value);
        }

        [DllImport("shell32.dll")]
        private static extern void SHChangeNotify(int eventId, uint flags, IntPtr item1, IntPtr item2);
    }
}
