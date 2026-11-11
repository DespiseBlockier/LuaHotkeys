using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace LuaHotkeys
{
    internal sealed class LuaWorker : IDisposable
    {
        private const int Registry = -10000;
        private const int Globals = -10002;
        private const int LuaNumber = 3;
        private const int LuaString = 4;
        private const int LuaFunction = 6;

        private readonly IntPtr state;
        private readonly List<LuaCallback> nativeCallbacks = new List<LuaCallback>();
        private readonly List<Binding> bindings = new List<Binding>();
        private readonly HashSet<uint> combinations = new HashSet<uint>();
        private int traceback;
        private bool loading = true;

        private sealed class Binding
        {
            internal string Name;
            internal KeyCombination Combination;
            internal int Reference;
        }

        private LuaWorker()
        {
            state = LuaNative.luaL_newstate();
            if (state == IntPtr.Zero)
                throw new InvalidOperationException("Lua could not allocate a runtime.");
        }

        internal static int Run()
        {
            try
            {
                string name = Wire.Decode(Console.ReadLine());
                string source = Wire.Decode(Console.ReadLine());
                var watchParent = new Thread(() =>
                {
                    try { Console.Read(); }
                    catch (IOException) { }
                    Environment.Exit(0);
                }) { IsBackground = true };
                watchParent.Start();

                using (var worker = new LuaWorker())
                {
                    worker.Initialize();
                    worker.RunScript(source, name);
                    worker.Listen();
                }
                return 0;
            }
            catch (Exception error)
            {
                Wire.Write("E", error.Message);
                return 1;
            }
        }

        private void Initialize()
        {
            LuaNative.luaL_openlibs(this.state);
            Load("return debug.traceback", "=(traceback)");
            Call(1, 0);
            traceback = LuaNative.luaL_ref(this.state, Registry);

            Export("hotkey", RegisterHotkey);
            Export("type_text", state => { WindowsInput.TypeText(StringAt(state, 1)); return 0; });
            Export("press", state => { WindowsInput.Press(KeyCombination.Parse(StringAt(state, 1))); return 0; });
            Export("click", state =>
            {
                WindowsInput.Click(LuaNative.lua_type(state, 1) <= 0 ? "left" : StringAt(state, 1));
                return 0;
            });
            Export("move_mouse", state => { WindowsInput.MoveMouse(IntegerAt(state, 1), IntegerAt(state, 2)); return 0; });
            Export("mouse_pos", state =>
            {
                var position = WindowsInput.MousePosition();
                LuaNative.lua_pushnumber(state, position.X);
                LuaNative.lua_pushnumber(state, position.Y);
                return 2;
            });
            Export("sleep", state =>
            {
                int milliseconds = IntegerAt(state, 1);
                if (milliseconds < 0)
                    throw new ArgumentOutOfRangeException("milliseconds", "Sleep requires a nonnegative duration.");
                Thread.Sleep(milliseconds);
                return 0;
            });
            Export("run", Launch);
            Export("write_log", state => { Wire.Write("L", StringAt(state, 1)); return 0; });

            Load(@"
                local function wrap(callback)
                    return function(...)
                        local ok, a, b = callback(...)
                        if not ok then error(a, 2) end
                        return a, b
                    end
                end
                for _, name in ipairs({
                    'hotkey', 'type_text', 'press', 'click', 'move_mouse',
                    'mouse_pos', 'sleep', 'run', 'write_log'
                }) do
                    _G[name] = wrap(_G['__luahotkeys_' .. name])
                    _G['__luahotkeys_' .. name] = nil
                end

                local write, to_string = write_log, tostring
                write_log = nil
                function log(...)
                    local parts = {}
                    for i = 1, select('#', ...) do
                        parts[i] = to_string(select(i, ...))
                    end
                    write(table.concat(parts, '\t'))
                end
                print = log
            ", "=(LuaHotkeys)");
            Call(0, 0);
        }

        private void Export(string name, Func<IntPtr, int> action)
        {
            LuaCallback callback = callbackState =>
            {
                try
                {
                    int values = action(callbackState);
                    LuaNative.lua_pushboolean(callbackState, 1);
                    LuaNative.lua_insert(callbackState, -1 - values);
                    return 1 + values;
                }
                catch (Exception error)
                {
                    LuaNative.lua_pushboolean(callbackState, 0);
                    PushString(callbackState, error.Message);
                    return 2;
                }
            };
            nativeCallbacks.Add(callback);
            LuaNative.lua_pushcclosure(state, callback, 0);
            LuaNative.lua_setfield(state, Globals, "__luahotkeys_" + name);
        }

        private int RegisterHotkey(IntPtr state)
        {
            if (!loading)
                throw new InvalidOperationException("Declare hotkeys when the script starts.");
            string name = StringAt(state, 1);
            KeyCombination combination = KeyCombination.Parse(name);
            if (combination.Key == 0x7B)
                throw new ArgumentException("Windows reserves F12. Choose a different key.");
            if (LuaNative.lua_type(state, 2) != LuaFunction)
                throw new ArgumentException("hotkey expects a Lua function as argument 2.");
            if (!combinations.Add((combination.Flags << 8) | combination.Key))
                throw new ArgumentException("Duplicate hotkey '" + name + "'.");

            LuaNative.lua_pushvalue(state, 2);
            bindings.Add(new Binding
            {
                Name = name,
                Combination = combination,
                Reference = LuaNative.luaL_ref(state, Registry)
            });
            return 0;
        }

        private void RunScript(string source, string name)
        {
            LuaNative.lua_rawgeti(state, Registry, traceback);
            Load(source, "@" + name);
            Call(0, 1);
            LuaNative.lua_settop(state, 0);
            loading = false;
        }

        private void Listen()
        {
            WindowsInput.Message message;
            WindowsInput.PeekMessage(out message, IntPtr.Zero, 0, 0, 0);
            int registered = 0;
            try
            {
                foreach (var binding in bindings)
                {
                    try { WindowsInput.Register(registered + 1, binding.Combination); }
                    catch (Win32Exception error)
                    {
                        throw new InvalidOperationException("Couldn't register " + binding.Name + ". " + error.Message);
                    }
                    registered++;
                }
                foreach (var binding in bindings)
                    Wire.Write("L", "Bound " + binding.Name);
                Wire.Write("R", registered.ToString());
                if (registered == 0)
                    return;

                int result;
                while ((result = WindowsInput.GetMessage(out message, IntPtr.Zero, WindowsInput.HotkeyMessage, WindowsInput.HotkeyMessage)) > 0)
                {
                    int index = (int)message.WParam.ToUInt32() - 1;
                    if (index < 0 || index >= bindings.Count)
                        continue;
                    InvokeBinding(bindings[index]);
                }
                if (result == -1)
                    throw new Win32Exception();
            }
            finally
            {
                for (int id = 1; id <= registered; id++)
                    WindowsInput.Unregister(id);
            }
        }

        private void InvokeBinding(Binding binding)
        {
            WindowsInput.WaitForRelease(binding.Combination);
            LuaNative.lua_rawgeti(state, Registry, traceback);
            LuaNative.lua_rawgeti(state, Registry, binding.Reference);
            try { Call(0, 1); }
            catch (InvalidOperationException error)
            {
                Wire.Write("E", binding.Name + " — " + error.Message);
            }
            finally { LuaNative.lua_settop(state, 0); }
        }

        private int Launch(IntPtr state)
        {
            string program = StringAt(state, 1);
            int count = LuaNative.lua_gettop(state) - 1;
            var arguments = Enumerable.Range(2, count).Select(index => QuoteArgument(StringAt(state, index)));
            var startInfo = new ProcessStartInfo(program, String.Join(" ", arguments))
            {
                UseShellExecute = true,
                WorkingDirectory = Environment.CurrentDirectory
            };
            using (Process.Start(startInfo)) { }
            return 0;
        }

        private static string QuoteArgument(string argument)
        {
            var result = new StringBuilder("\"");
            int slashes = 0;
            foreach (char character in argument)
            {
                if (character == '\\')
                {
                    slashes++;
                    continue;
                }
                if (character == '"')
                    result.Append('\\', slashes + 1);
                result.Append('\\', slashes).Append(character);
                slashes = 0;
            }
            return result.Append('\\', slashes * 2).Append('"').ToString();
        }

        private static int IntegerAt(IntPtr state, int index)
        {
            if (LuaNative.lua_type(state, index) == LuaNumber)
            {
                double number = LuaNative.lua_tonumber(state, index);
                if (number >= Int32.MinValue && number <= Int32.MaxValue && number == Math.Truncate(number))
                    return (int)number;
            }
            throw new ArgumentException("Argument " + index + " must be an integer.");
        }

        private static string StringAt(IntPtr state, int index)
        {
            if (LuaNative.lua_type(state, index) != LuaString)
                throw new ArgumentException("Argument " + index + " must be a string.");
            return ReadString(state, index);
        }

        private static string ReadString(IntPtr state, int index)
        {
            UIntPtr length;
            IntPtr pointer = LuaNative.lua_tolstring(state, index, out length);
            if (pointer == IntPtr.Zero)
                return "Lua raised a non-string error.";
            byte[] bytes = new byte[length.ToUInt32()];
            Marshal.Copy(pointer, bytes, 0, bytes.Length);
            return Encoding.UTF8.GetString(bytes);
        }

        private static void PushString(IntPtr state, string value)
        {
            byte[] bytes = Encoding.UTF8.GetBytes(value);
            LuaNative.lua_pushlstring(state, bytes, (UIntPtr)bytes.Length);
        }

        private void Load(string source, string name)
        {
            byte[] bytes = Encoding.UTF8.GetBytes(source);
            byte[] chunk = Encoding.UTF8.GetBytes(name + "\0");
            if (LuaNative.luaL_loadbuffer(state, bytes, (UIntPtr)bytes.Length, chunk) != 0)
                throw new InvalidOperationException(ReadString(state, -1));
        }

        private void Call(int results, int errorHandler)
        {
            if (LuaNative.lua_pcall(state, 0, results, errorHandler) != 0)
                throw new InvalidOperationException(ReadString(state, -1));
        }

        public void Dispose()
        {
            LuaNative.lua_close(state);
            GC.KeepAlive(nativeCallbacks);
        }

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int LuaCallback(IntPtr state);

        private static class LuaNative
        {
            private const string Dll = "lua5.1.dll";
            private const CallingConvention Cdecl = CallingConvention.Cdecl;
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern IntPtr luaL_newstate();
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_close(IntPtr state);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void luaL_openlibs(IntPtr state);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern int luaL_loadbuffer(IntPtr state, byte[] source, UIntPtr length, byte[] name);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern int lua_pcall(IntPtr state, int arguments, int results, int errorHandler);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern int luaL_ref(IntPtr state, int table);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_rawgeti(IntPtr state, int table, int reference);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_setfield(IntPtr state, int table, string name);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_settop(IntPtr state, int index);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern int lua_gettop(IntPtr state);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_insert(IntPtr state, int index);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern int lua_type(IntPtr state, int index);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_pushcclosure(IntPtr state, LuaCallback callback, int upvalues);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_pushvalue(IntPtr state, int index);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_pushboolean(IntPtr state, int value);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_pushnumber(IntPtr state, double value);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern void lua_pushlstring(IntPtr state, byte[] value, UIntPtr length);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern double lua_tonumber(IntPtr state, int index);
            [DllImport(Dll, CallingConvention = Cdecl)] internal static extern IntPtr lua_tolstring(IntPtr state, int index, out UIntPtr length);
        }
    }

    internal static class Wire
    {
        internal static string Encode(string text)
        {
            return Convert.ToBase64String(Encoding.UTF8.GetBytes(text));
        }

        internal static string Decode(string text)
        {
            return Encoding.UTF8.GetString(Convert.FromBase64String(text));
        }

        internal static void Write(string type, string message)
        {
            const int limit = 12000;
            if (message.Length > limit)
                message = message.Substring(0, limit) + "…";
            Console.WriteLine(type + " " + Encode(message));
        }
    }
}
