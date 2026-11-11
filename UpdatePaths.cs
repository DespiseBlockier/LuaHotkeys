using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;

namespace LuaHotkeys
{
    internal static class UpdatePaths
    {
        internal static string Normalize(string folder)
        {
            string full = Path.GetFullPath(folder);
            return full.Length == Path.GetPathRoot(full).Length ? full : full.TrimEnd('\\', '/');
        }

        internal static string MutexName(string folder)
        {
            using (var hash = SHA256.Create())
            {
                byte[] path = Encoding.UTF8.GetBytes(Normalize(folder).ToUpperInvariant());
                return @"Local\LuaHotkeys.Update." + BitConverter.ToString(hash.ComputeHash(path)).Replace("-", "");
            }
        }

        internal static string CloseEventName(int processId)
        {
            return @"Local\LuaHotkeys.CloseForUpdate." + processId;
        }

        internal static bool IsUpdating(string folder)
        {
            Mutex mutex;
            if (!Mutex.TryOpenExisting(MutexName(folder), out mutex))
                return false;
            using (mutex)
            {
                if (!TryAcquire(mutex))
                    return true;
                mutex.ReleaseMutex();
                return false;
            }
        }

        internal static bool TryAcquire(Mutex mutex)
        {
            try { return mutex.WaitOne(0); }
            catch (AbandonedMutexException) { return true; }
        }
    }
}
