# Norma OS Browser Session Bridge

This extension responds only to the bundled `com.norma.os.browser_auth` Native Messaging host. It reads cookies only for an exact HTTP(S) URL requested after the user clicks the Chrome session button in a Norma Browser card.

For development and locally packaged builds:

1. Open `chrome://extensions` in Google Chrome.
2. Enable Developer mode.
3. Choose **Load unpacked** and select this directory. If the connector is missing, Norma's error banner prints the exact bundled `Contents/Resources/browser-auth-extension` path.
4. Build `norma-browser-auth-host` next to the `norma-os` executable, then click the Browser card's Chrome button once so Norma registers the user-level Native Messaging manifest.

The fixed manifest public key produces extension ID `fkhlnglmgbebphicanlcckhpflbdfopf`. Cookie values are never logged by the extension or helper.
