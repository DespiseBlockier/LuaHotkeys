using System;
using System.IO;
using System.IO.Compression;
using System.Linq;

namespace LuaHotkeys
{
    internal static class PortableUpdate
    {
        private const string Marker = "LuaHotkeys portable update";
        private static readonly string[] ProgramFiles =
        {
            "lua5.1.dll", "Lua-LICENSE.txt", "LuaHotkeysUpdater.exe", "LuaHotkeys.exe"
        };

        internal static string StagingFolder(string folder)
        {
            return Path.Combine(folder, ".luahotkeys-update");
        }

        internal static void Recover(string folder)
        {
            string stage = StagingFolder(folder);
            if (!Directory.Exists(stage))
                return;
            if ((File.GetAttributes(stage) & FileAttributes.ReparsePoint) != 0)
                throw new IOException("The update staging folder is a link. Check " + stage);
            string pending = Path.Combine(stage, "pending");
            if (!File.Exists(pending))
            {
                if (Directory.EnumerateFileSystemEntries(stage).Any())
                    throw new IOException("The update staging folder already exists. Check " + stage);
                Directory.Delete(stage);
                return;
            }
            if (File.ReadAllText(pending) != Marker)
                throw new IOException("Unrecognized update staging folder. Check " + stage);

            if (!File.Exists(Path.Combine(stage, "complete")))
            {
                foreach (string name in ProgramFiles.Reverse())
                {
                    string destination = Path.Combine(folder, name);
                    string backup = Path.Combine(stage, name + ".old");
                    if (File.Exists(backup))
                    {
                        if (File.Exists(destination))
                            File.Replace(backup, destination, null);
                        else
                            File.Move(backup, destination);
                    }
                    else if (File.Exists(Path.Combine(stage, name + ".absent")))
                    {
                        File.Delete(destination);
                    }
                }
            }

            foreach (string name in ProgramFiles)
            {
                File.Delete(Path.Combine(stage, name));
                File.Delete(Path.Combine(stage, name + ".old"));
                File.Delete(Path.Combine(stage, name + ".absent"));
            }
            File.Delete(Path.Combine(stage, "complete"));
            File.Delete(pending);
            Directory.Delete(stage);
        }

        internal static void Apply(string folder, string package, Version version)
        {
            string stage = StagingFolder(folder);
            if (Directory.Exists(stage))
                throw new IOException("Recover the previous portable update before applying another.");
            Directory.CreateDirectory(stage);
            File.WriteAllText(Path.Combine(stage, "pending"), Marker);
            try
            {
                ExtractProgramFiles(package, stage);
                if (UpdateInstaller.FileVersion(Path.Combine(stage, "LuaHotkeys.exe")) != version ||
                    UpdateInstaller.FileVersion(Path.Combine(stage, "LuaHotkeysUpdater.exe")) != version)
                    throw new InvalidDataException("The portable build does not match the release version.");

                foreach (string name in ProgramFiles)
                {
                    string destination = Path.Combine(folder, name);
                    string source = Path.Combine(stage, name);
                    if (File.Exists(destination))
                        File.Replace(source, destination, Path.Combine(stage, name + ".old"));
                    else
                    {
                        File.WriteAllText(Path.Combine(stage, name + ".absent"), Marker);
                        File.Move(source, destination);
                    }
                }
                File.WriteAllText(Path.Combine(stage, "complete"), Marker);
            }
            catch
            {
                try { Recover(folder); }
                catch (Exception recoveryError)
                {
                    throw new IOException("Could not restore the previous build. Its files are backed up in " + stage, recoveryError);
                }
                throw;
            }
            Recover(folder);
        }

        private static void ExtractProgramFiles(string package, string stage)
        {
            using (var file = File.OpenRead(package))
            using (var archive = new ZipArchive(file, ZipArchiveMode.Read))
            {
                foreach (string name in ProgramFiles)
                {
                    ZipArchiveEntry[] matches = archive.Entries.Where(entry => entry.FullName == name).ToArray();
                    if (matches.Length != 1 || matches[0].Length <= 0 || matches[0].Length > GitHubRelease.MaxAssetSize)
                        throw new InvalidDataException("The portable update has an invalid " + name + " file.");
                    using (var source = matches[0].Open())
                    using (var output = new FileStream(Path.Combine(stage, name), FileMode.CreateNew))
                        source.CopyTo(output);
                }
            }
        }
    }
}
