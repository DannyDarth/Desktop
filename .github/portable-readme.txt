OpenMouse Desktop - portable Windows test build

1. Extract all files from this ZIP into a folder.
2. Run OpenMouseDesktop.exe. No installer is needed.
3. Before replacing a build, choose Quit from the OpenMouse tray icon.
   Closing the window only hides it to the tray.
4. Extract the next build and run its OpenMouseDesktop.exe.

Keep only one instance running when testing macros. Quit an installed copy
before launching this portable build.

This is an install-free executable. Your settings and macros stay in the
same per-user configuration folder used by the installer:
  %APPDATA%\openmouse

The folder includes settings.json, macros.json and game-profiles.json.
Replacing the executable preserves that data. It will also be available when
you later install this app under the same Windows account. The ZIP does not
carry your personal settings to another computer. You can back up the
configuration folder while the app is closed.

Do not enable Launch on startup for a temporary testing folder; Windows would
keep pointing at that location after you move or remove the portable build.

BUILD.txt identifies the source commit used for this build.
