# LuaHotkeys releases

Implement each update, run development tests, and give the user the development launcher so they can try the working tree. Wait for explicit approval for that update before creating release builds. Then build, validate, and publish the release.

Include both the Windows MSI installer and the portable ZIP in each GitHub release.

Once the user approves publication, publish and verify the Windows release before starting Linux build, packaging, or repository work. Publish the Linux installer and source archive and update the signed Linux repositories afterward.
