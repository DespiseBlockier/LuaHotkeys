using System;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;
using System.Windows.Forms;

namespace LuaHotkeys
{
    internal sealed class LuaEditor : RichTextBox
    {
        private static readonly Color[] Palette =
        {
            Color.FromArgb(200, 213, 236),
            Color.FromArgb(186, 166, 255),
            Color.FromArgb(151, 224, 180),
            Color.FromArgb(247, 196, 122),
            Color.FromArgb(121, 207, 255)
        };

        private const int GetOleInterface = 0x043C;
        private const int GetScrollPosition = 0x04DD;
        private const int SetScrollPosition = 0x04DE;
        private const int SetRedraw = 0x000B;
        private const int SuspendUndo = -9999995;
        private const int ResumeUndo = -9999994;
        private readonly Timer recolor = new Timer { Interval = 110 };
        private bool paintingSyntax;

        internal LuaEditor()
        {
            ForeColor = Color.FromArgb(234, 240, 252);
            recolor.Tick += (sender, args) =>
            {
                recolor.Stop();
                ColorScript();
            };
        }

        protected override void OnTextChanged(EventArgs args)
        {
            base.OnTextChanged(args);
            if (!paintingSyntax && IsHandleCreated)
            {
                recolor.Stop();
                recolor.Start();
            }
        }

        protected override void OnHandleCreated(EventArgs args)
        {
            base.OnHandleCreated(args);
            recolor.Start();
        }

        protected override void OnHandleDestroyed(EventArgs args)
        {
            recolor.Stop();
            base.OnHandleDestroyed(args);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
                recolor.Stop();
            base.Dispose(disposing);
            if (disposing)
                recolor.Dispose();
        }

        internal void ColorScript()
        {
            IntPtr ole;
            if (SendMessage(Handle, GetOleInterface, IntPtr.Zero, out ole) == IntPtr.Zero || ole == IntPtr.Zero)
                return;

            IntPtr document;
            Guid documentId = new Guid("8CC497C0-A1DF-11CE-8098-00AA0047BE5D");
            try
            {
                if (Marshal.QueryInterface(ole, ref documentId, out document) != 0)
                    return;
            }
            finally
            {
                Marshal.Release(ole);
            }

            try
            {
                // ITextDocument::Undo is entry 22 in the TOM interface declared in tom.h.
                IntPtr vtable = Marshal.ReadIntPtr(document);
                IntPtr undoAddress = Marshal.ReadIntPtr(vtable, 22 * IntPtr.Size);
                var undo = (UndoOperation)Marshal.GetDelegateForFunctionPointer(undoAddress, typeof(UndoOperation));
                if (undo(document, SuspendUndo, IntPtr.Zero) < 0)
                    return;
                try { ApplyColors(); }
                finally { undo(document, ResumeUndo, IntPtr.Zero); }
            }
            finally
            {
                Marshal.Release(document);
            }
        }

        private void ApplyColors()
        {
            string source = Text;
            if (source.Length == 0)
            {
                SelectionColor = ForeColor;
                return;
            }

            int selectionStart = SelectionStart;
            int selectionLength = SelectionLength;
            var scroll = new ScrollPosition();
            bool redraw = Visible;
            SendMessage(Handle, GetScrollPosition, IntPtr.Zero, ref scroll);
            if (redraw)
                SendMessage(Handle, SetRedraw, IntPtr.Zero, IntPtr.Zero);
            paintingSyntax = true;
            try
            {
                SelectAll();
                SelectionColor = ForeColor;
                foreach (var token in LuaSyntax.Scan(source))
                {
                    Select(token.Start, token.Length);
                    SelectionColor = Palette[(int)token.Kind];
                }
            }
            finally
            {
                Select(selectionStart, selectionLength);
                SendMessage(Handle, SetScrollPosition, IntPtr.Zero, ref scroll);
                if (redraw)
                {
                    SendMessage(Handle, SetRedraw, new IntPtr(1), IntPtr.Zero);
                    Invalidate();
                }
                paintingSyntax = false;
            }
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct ScrollPosition { internal int X, Y; }

        [UnmanagedFunctionPointer(CallingConvention.StdCall)]
        private delegate int UndoOperation(IntPtr document, int count, IntPtr actualCount);

        [DllImport("user32.dll", EntryPoint = "SendMessageW")]
        private static extern IntPtr SendMessage(IntPtr window, int message, IntPtr wparam, IntPtr lparam);
        [DllImport("user32.dll", EntryPoint = "SendMessageW")]
        private static extern IntPtr SendMessage(IntPtr window, int message, IntPtr wparam, ref ScrollPosition lparam);
        [DllImport("user32.dll", EntryPoint = "SendMessageW")]
        private static extern IntPtr SendMessage(IntPtr window, int message, IntPtr wparam, out IntPtr lparam);
    }

    internal enum TokenKind { Comment, Keyword, String, Number, Function }

    internal struct LuaToken
    {
        internal readonly int Start, Length;
        internal readonly TokenKind Kind;

        internal LuaToken(int start, int end, TokenKind kind)
        {
            Start = start;
            Length = end - start;
            Kind = kind;
        }
    }

    internal static class LuaSyntax
    {
        private static readonly HashSet<string> Keywords = new HashSet<string>(StringComparer.Ordinal)
        {
            "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in",
            "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while"
        };

        private static readonly HashSet<string> Functions = new HashSet<string>(StringComparer.Ordinal)
        {
            "hotkey", "type_text", "press", "click", "move_mouse", "mouse_pos", "sleep", "run", "log",
            "print", "assert", "error", "ipairs", "pairs", "pcall", "require", "tonumber", "tostring", "type", "xpcall"
        };

        private static readonly Regex Number = new Regex(
            @"\G(?:0[xX][0-9a-fA-F]+|(?:[0-9]+(?:\.(?!\.)[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?)",
            RegexOptions.CultureInvariant);

        internal static IEnumerable<LuaToken> Scan(string source)
        {
            int position = 0;
            while (position < source.Length)
            {
                int start = position;
                char character = source[position++];
                char next = position < source.Length ? source[position] : '\0';

                if (character == '-' && next == '-')
                {
                    position++;
                    if (!ReadLongBracket(source, ref position))
                        while (position < source.Length && source[position] != '\n' && source[position] != '\r')
                            position++;
                    yield return new LuaToken(start, position, TokenKind.Comment);
                }
                else if (character == '[')
                {
                    int end = start;
                    if (ReadLongBracket(source, ref end))
                    {
                        position = end;
                        yield return new LuaToken(start, position, TokenKind.String);
                    }
                }
                else if (character == '"' || character == '\'')
                {
                    position = ReadQuotedString(source, position, character);
                    yield return new LuaToken(start, position, TokenKind.String);
                }
                else if (IsLetter(character))
                {
                    while (position < source.Length && (IsLetter(source[position]) || IsDigit(source[position])))
                        position++;
                    string identifier = source.Substring(start, position - start);
                    if (Keywords.Contains(identifier))
                        yield return new LuaToken(start, position, TokenKind.Keyword);
                    else if (Functions.Contains(identifier))
                        yield return new LuaToken(start, position, TokenKind.Function);
                }
                else if (character == '.' && next == '.')
                {
                    while (position < source.Length && source[position] == '.')
                        position++;
                }
                else if (IsDigit(character) || (character == '.' && IsDigit(next)))
                {
                    Match match = Number.Match(source, start);
                    position = start + match.Length;
                    yield return new LuaToken(start, position, TokenKind.Number);
                }
            }
        }

        private static int ReadQuotedString(string source, int position, char quote)
        {
            while (position < source.Length)
            {
                char character = source[position];
                if (character == '\r' || character == '\n')
                    return position;
                position++;
                if (character == quote)
                    break;
                if (character == '\\' && position < source.Length)
                {
                    char escaped = source[position++];
                    if (escaped == '\r' && position < source.Length && source[position] == '\n')
                        position++;
                }
            }
            return position;
        }

        private static bool ReadLongBracket(string source, ref int position)
        {
            if (position >= source.Length || source[position] != '[')
                return false;
            int opening = position + 1;
            while (opening < source.Length && source[opening] == '=')
                opening++;
            if (opening >= source.Length || source[opening] != '[')
                return false;

            string closing = "]" + new string('=', opening - position - 1) + "]";
            int end = source.IndexOf(closing, opening + 1, StringComparison.Ordinal);
            position = end < 0 ? source.Length : end + closing.Length;
            return true;
        }

        private static bool IsLetter(char character)
        {
            return (character >= 'A' && character <= 'Z') || (character >= 'a' && character <= 'z') || character == '_';
        }

        private static bool IsDigit(char character)
        {
            return character >= '0' && character <= '9';
        }
    }
}
