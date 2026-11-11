using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;

namespace LuaHotkeys
{
    internal sealed class KeyCombination
    {
        private static readonly Dictionary<string, uint> KeysByName = CreateKeys();
        private static readonly Dictionary<string, uint> Modifiers = new Dictionary<string, uint>(StringComparer.OrdinalIgnoreCase)
        {
            { "Alt", 1 }, { "Ctrl", 2 }, { "Control", 2 }, { "Shift", 4 }, { "Win", 8 }, { "Windows", 8 }
        };
        private static readonly uint[] ModifierFlags = { 2, 1, 4, 8 };
        private static readonly uint[] ModifierKeys = { 0x11, 0x12, 0x10, 0x5B };
        internal readonly uint Flags;
        internal readonly uint Key;

        private KeyCombination(uint flags, uint key)
        {
            Flags = flags;
            Key = key;
        }

        internal static KeyCombination Parse(string text)
        {
            string[] parts = text.Split('+');
            uint flags = 0;
            for (int i = 0; i < parts.Length - 1; i++)
            {
                uint flag;
                if (!Modifiers.TryGetValue(parts[i].Trim(), out flag) || (flags & flag) != 0)
                    throw new ArgumentException("Invalid modifier in '" + text + "'. Use Ctrl, Alt, Shift or Win.");
                flags |= flag;
            }
            uint key;
            if (!KeysByName.TryGetValue(parts[parts.Length - 1].Trim(), out key))
                throw new ArgumentException("Unknown key in '" + text + "'.");
            return new KeyCombination(flags, key);
        }

        internal IEnumerable<uint> ModifierKeyCodes()
        {
            for (int i = 0; i < ModifierFlags.Length; i++)
                if ((Flags & ModifierFlags[i]) != 0)
                    yield return ModifierKeys[i];
        }

        private static Dictionary<string, uint> CreateKeys()
        {
            var keys = new Dictionary<string, uint>(StringComparer.OrdinalIgnoreCase);
            for (char key = 'A'; key <= 'Z'; key++)
                keys.Add(key.ToString(), key);
            for (char key = '0'; key <= '9'; key++)
                keys.Add(key.ToString(), key);
            for (uint index = 1; index <= 24; index++)
                keys.Add("F" + index, 0x6F + index);
            for (uint index = 0; index <= 9; index++)
                keys.Add("NumPad" + index, 0x60 + index);
            var named = new Dictionary<string, Keys>
            {
                { "Enter", Keys.Enter }, { "Esc", Keys.Escape }, { "Escape", Keys.Escape },
                { "Space", Keys.Space }, { "Tab", Keys.Tab }, { "Backspace", Keys.Back },
                { "Delete", Keys.Delete }, { "Insert", Keys.Insert }, { "Home", Keys.Home },
                { "End", Keys.End }, { "PageUp", Keys.PageUp }, { "PageDown", Keys.PageDown },
                { "Up", Keys.Up }, { "Down", Keys.Down }, { "Left", Keys.Left }, { "Right", Keys.Right },
                { "CapsLock", Keys.CapsLock }, { "PrintScreen", Keys.PrintScreen },
                { "VolumeUp", Keys.VolumeUp }, { "VolumeDown", Keys.VolumeDown },
                { "VolumeMute", Keys.VolumeMute }, { "MediaPlayPause", Keys.MediaPlayPause }
            };
            foreach (var entry in named)
                keys.Add(entry.Key, (uint)entry.Value);
            return keys;
        }
    }

    internal static class WindowsInput
    {
        internal const uint HotkeyMessage = 0x0312;
        private const uint NoRepeat = 0x4000;
        private const uint ExtendedKey = 1;
        private const uint KeyUp = 2;
        private const uint Unicode = 4;
        private static readonly HashSet<uint> ExtendedKeys = new HashSet<uint>
        {
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2C, 0x2D, 0x2E, 0x5B,
            0xAD, 0xAE, 0xAF, 0xB3
        };
        private static readonly Dictionary<string, uint> MouseButtons = new Dictionary<string, uint>(StringComparer.OrdinalIgnoreCase)
        {
            { "left", 2 }, { "right", 8 }, { "middle", 32 }
        };

        internal static void Register(int id, KeyCombination combination)
        {
            if (!RegisterHotKey(IntPtr.Zero, id, combination.Flags | NoRepeat, combination.Key))
                throw new Win32Exception();
        }

        internal static void Unregister(int id)
        {
            UnregisterHotKey(IntPtr.Zero, id);
        }

        internal static void WaitForRelease(KeyCombination combination)
        {
            uint[] keys = combination.ModifierKeyCodes().Concat(new[] { combination.Key }).ToArray();
            while (keys.Any(IsDown))
                Thread.Sleep(10);
        }

        internal static void Press(KeyCombination combination)
        {
            uint[] modifiers = combination.ModifierKeyCodes().Where(key => !IsDown(key)).ToArray();
            var inputs = new List<Input>();
            foreach (uint modifier in modifiers)
                inputs.Add(KeyEvent(modifier, 0, 0));
            inputs.Add(KeyEvent(combination.Key, 0, 0));
            inputs.Add(KeyEvent(combination.Key, 0, KeyUp));
            for (int i = modifiers.Length - 1; i >= 0; i--)
                inputs.Add(KeyEvent(modifiers[i], 0, KeyUp));
            Send(inputs.ToArray());
        }

        internal static void TypeText(string text)
        {
            var inputs = new List<Input>();
            for (int i = 0; i < text.Length; i++)
            {
                char character = text[i];
                if (character == '\r' && i + 1 < text.Length && text[i + 1] == '\n')
                    continue;
                if (character == '\r' || character == '\n')
                {
                    inputs.Add(KeyEvent((uint)Keys.Enter, 0, 0));
                    inputs.Add(KeyEvent((uint)Keys.Enter, 0, KeyUp));
                }
                else
                {
                    inputs.Add(KeyEvent(0, character, Unicode));
                    inputs.Add(KeyEvent(0, character, Unicode | KeyUp));
                }
            }
            Send(inputs.ToArray());
        }

        internal static void MoveMouse(int x, int y)
        {
            if (!SetCursorPos(x, y))
                throw new Win32Exception();
        }

        internal static Point MousePosition()
        {
            Point point;
            if (!GetCursorPos(out point))
                throw new Win32Exception();
            return point;
        }

        internal static void Click(string button)
        {
            uint down;
            if (!MouseButtons.TryGetValue(button, out down))
                throw new ArgumentException("Mouse button must be left, right or middle.");
            Send(new[]
            {
                new Input { Type = 0, Data = new InputData { Mouse = new MouseInput { Flags = down } } },
                new Input { Type = 0, Data = new InputData { Mouse = new MouseInput { Flags = down << 1 } } }
            });
        }

        private static bool IsDown(uint key)
        {
            if (key == 0x5B)
                return GetAsyncKeyState(0x5B) < 0 || GetAsyncKeyState(0x5C) < 0;
            return GetAsyncKeyState((int)key) < 0;
        }

        private static Input KeyEvent(uint key, ushort scan, uint flags)
        {
            if (ExtendedKeys.Contains(key))
                flags |= ExtendedKey;
            return new Input
            {
                Type = 1,
                Data = new InputData { Keyboard = new KeyboardInput { Key = (ushort)key, Scan = scan, Flags = flags } }
            };
        }

        private static void Send(Input[] inputs)
        {
            if (inputs.Length == 0)
                return;
            if (SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(Input))) != inputs.Length)
                throw new InvalidOperationException("Windows rejected the input. Check the focused window and its privilege level.");
        }

        [StructLayout(LayoutKind.Sequential)]
        internal struct Point { internal int X, Y; }

        [StructLayout(LayoutKind.Sequential)]
        internal struct Message
        {
            internal IntPtr Window;
            internal uint Id;
            internal UIntPtr WParam;
            internal IntPtr LParam;
            internal uint Time;
            internal Point Position;
            internal uint Private;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct Input { internal uint Type; internal InputData Data; }

        [StructLayout(LayoutKind.Explicit)]
        private struct InputData
        {
            [FieldOffset(0)] internal KeyboardInput Keyboard;
            [FieldOffset(0)] internal MouseInput Mouse;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct KeyboardInput
        {
            internal ushort Key, Scan;
            internal uint Flags, Time;
            internal UIntPtr Extra;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct MouseInput
        {
            internal int X, Y;
            internal uint Data, Flags, Time;
            internal UIntPtr Extra;
        }

        [DllImport("user32.dll")]
        internal static extern bool SetProcessDPIAware();
        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool RegisterHotKey(IntPtr hwnd, int id, uint flags, uint key);
        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool UnregisterHotKey(IntPtr hwnd, int id);
        [DllImport("user32.dll", EntryPoint = "PeekMessageW")]
        internal static extern bool PeekMessage(out Message message, IntPtr hwnd, uint min, uint max, uint remove);
        [DllImport("user32.dll", EntryPoint = "GetMessageW", SetLastError = true)]
        internal static extern int GetMessage(out Message message, IntPtr hwnd, uint min, uint max);
        [DllImport("user32.dll")]
        private static extern short GetAsyncKeyState(int key);
        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool SetCursorPos(int x, int y);
        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool GetCursorPos(out Point point);
        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint SendInput(uint count, Input[] input, int inputSize);
    }
}
