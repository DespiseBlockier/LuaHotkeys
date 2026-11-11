using System;
using System.IO;
using System.Linq;
using System.Net;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;
using System.Security.Cryptography;
using System.Text.RegularExpressions;

namespace LuaHotkeys
{
    [DataContract]
    internal sealed class ReleaseAsset
    {
        [DataMember(Name = "name")] internal string Name { get; set; }
        [DataMember(Name = "size")] internal long Size { get; set; }
        [DataMember(Name = "digest")] internal string Digest { get; set; }
        [DataMember(Name = "state")] internal string State { get; set; }
    }

    internal sealed class GitHubRelease
    {
        internal const long MaxAssetSize = 32 * 1024 * 1024;
        private const string Repository = "DespiseBlockier/LuaHotkeys";
        private static readonly Regex Sha256 = new Regex(@"\Asha256:[0-9a-f]{64}\z", RegexOptions.CultureInvariant);

        [DataContract]
        private sealed class Release
        {
            [DataMember(Name = "tag_name")] internal string Tag { get; set; }
            [DataMember(Name = "draft")] internal bool Draft { get; set; }
            [DataMember(Name = "prerelease")] internal bool Prerelease { get; set; }
            [DataMember(Name = "assets")] internal ReleaseAsset[] Assets { get; set; }
        }

        internal readonly Version Version;
        internal readonly ReleaseAsset Asset;

        private GitHubRelease(Version version, ReleaseAsset asset)
        {
            Version = version;
            Asset = asset;
        }

        internal static GitHubRelease Check(Version current, bool installed)
        {
            // Use the operating system's HTTPS protocol and certificate settings.
            ServicePointManager.SecurityProtocol = SecurityProtocolType.SystemDefault;
            var uri = new Uri("https://api.github.com/repos/" + Repository + "/releases/latest");
            using (var response = Open(uri, true))
            using (var stream = response.GetResponseStream())
                return Parse(stream, current, installed);
        }

        internal static GitHubRelease Parse(Stream json, Version current, bool installed)
        {
            var release = (Release)new DataContractJsonSerializer(typeof(Release)).ReadObject(json);
            Version version;
            if (release == null || release.Draft || release.Prerelease || String.IsNullOrEmpty(release.Tag) ||
                release.Tag[0] != 'v' || !Version.TryParse(release.Tag.Substring(1), out version) ||
                version.Build < 0 || version.Revision >= 0 || release.Tag != "v" + version.ToString(3))
                throw new InvalidDataException("GitHub returned an invalid release version.");
            if (version <= current)
                return new GitHubRelease(version, null);

            string file = "LuaHotkeys-" + version.ToString(3) + (installed ? "-x86.msi" : "-win-x86.zip");
            ReleaseAsset[] assets = (release.Assets ?? new ReleaseAsset[0]).Where(asset => asset.Name == file).ToArray();
            if (assets.Length != 1 || assets[0].State != "uploaded" || assets[0].Size <= 0 ||
                assets[0].Size > MaxAssetSize || !Sha256.IsMatch(assets[0].Digest ?? ""))
                throw new InvalidDataException("The latest release does not have a verified " + file + " download.");
            return new GitHubRelease(version, assets[0]);
        }

        internal void Download(string path)
        {
            var uri = new Uri("https://github.com/" + Repository + "/releases/download/v" + Version.ToString(3) + "/" + Asset.Name);
            using (var response = Open(uri, false))
            using (var remote = response.GetResponseStream())
            using (var local = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.None))
            {
                if (response.ContentLength >= 0 && response.ContentLength != Asset.Size)
                    throw new InvalidDataException("The update download has an unexpected size.");
                var buffer = new byte[81920];
                long received = 0;
                int read;
                while ((read = remote.Read(buffer, 0, buffer.Length)) != 0)
                {
                    received += read;
                    if (received > Asset.Size)
                        throw new InvalidDataException("The update download has an unexpected size.");
                    local.Write(buffer, 0, read);
                }
                if (received != Asset.Size)
                    throw new InvalidDataException("The update download was incomplete.");
            }
            VerifyDownload(path, Asset);
        }

        internal static void VerifyDownload(string path, ReleaseAsset asset)
        {
            using (var stream = File.OpenRead(path))
            using (var hash = SHA256.Create())
            {
                string actual = "sha256:" + BitConverter.ToString(hash.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
                if (stream.Length != asset.Size || !String.Equals(actual, asset.Digest, StringComparison.Ordinal))
                    throw new InvalidDataException("The update download failed SHA-256 verification.");
            }
        }

        private static HttpWebResponse Open(Uri uri, bool api)
        {
            for (int redirects = 0; redirects <= 5; redirects++)
            {
                if (uri.Scheme != Uri.UriSchemeHttps || !uri.IsDefaultPort || uri.UserInfo.Length != 0 ||
                    (api ? uri.Host != "api.github.com" : !IsDownloadHost(uri.Host)))
                    throw new InvalidDataException("GitHub returned an unexpected download location.");

                var request = (HttpWebRequest)WebRequest.Create(uri);
                request.AllowAutoRedirect = false;
                request.UserAgent = "LuaHotkeys-Updater";
                if (api) request.Headers["X-GitHub-Api-Version"] = "2022-11-28";
                request.Accept = api ? "application/vnd.github+json" : "application/octet-stream";
                request.Timeout = 20000;
                request.ReadWriteTimeout = 20000;
                var response = (HttpWebResponse)request.GetResponse();
                int status = (int)response.StatusCode;
                if (status == 200)
                    return response;
                string location = response.Headers["Location"];
                response.Dispose();
                if (status < 300 || status >= 400 || String.IsNullOrEmpty(location) || !Uri.TryCreate(uri, location, out uri))
                    break;
            }
            throw new InvalidDataException("GitHub did not return an update download.");
        }

        private static bool IsDownloadHost(string host)
        {
            return host == "github.com" || host == "release-assets.githubusercontent.com" ||
                   host == "objects.githubusercontent.com" || host == "github-releases.githubusercontent.com";
        }
    }
}
