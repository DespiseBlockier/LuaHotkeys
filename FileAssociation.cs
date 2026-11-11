using System;
using System.Runtime.InteropServices;
using Microsoft.Win32;

namespace LuaHotkeys
{
    internal static class FileAssociation
    {
        internal const string Extension = ".luahotkeys";
        private const string ProgramId = "LuaHotkeys.Script";

        internal static void Register(string executable)
        {
            using (RegistryKey classes = Registry.CurrentUser.CreateSubKey(@"Software\Classes"))
            {
                WriteDefault(classes, ProgramId, "LuaHotkeys script");
                WriteDefault(classes, ProgramId + @"\DefaultIcon", "\"" + executable + "\",0");
                WriteDefault(classes, ProgramId + @"\shell", "open");
                RegisterVerb(classes, executable, "open", "Run with LuaHotkeys", "--run");
                RegisterVerb(classes, executable, "edit", "Edit in LuaHotkeys", "--edit");
                using (RegistryKey openWith = classes.CreateSubKey(Extension + @"\OpenWithProgids"))
                    openWith.SetValue(ProgramId, new byte[0], RegistryValueKind.None);
                WriteDefault(classes, Extension, ProgramId);
            }
            const int associationChanged = 0x08000000;
            SHChangeNotify(associationChanged, 0, IntPtr.Zero, IntPtr.Zero);
        }

        private static void RegisterVerb(RegistryKey classes, string executable, string verb, string label, string option)
        {
            string key = ProgramId + @"\shell\" + verb;
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
