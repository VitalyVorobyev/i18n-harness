# Install the app

The desktop app is published as installers on the project's
[GitHub releases page](https://github.com/VitalyVorobyev/i18n-harness/releases).
Each release lists files for macOS, Windows and Linux. Download the one
for your system:

| System | File to download |
|---|---|
| macOS, Apple silicon | the `.dmg` whose name contains `aarch64` |
| macOS, Intel | the `.dmg` whose name contains `x64` |
| Windows 10 / 11 | the `-setup.exe` (or the `.msi` if your organisation deploys MSI packages) |
| Linux (Debian, Ubuntu) | the `.deb` |
| Linux (other distributions) | the `.AppImage` |

The same release also has archives named `i18n-harness-cli-…` with the
command-line tools `harness` and `harness-lab`, plus a `.sha256` checksum
for each archive. You only need them for the command-line tutorials.

> **The builds are not signed by a known publisher** (unless a release
> note says otherwise). Your system will warn you the first time you open
> the app. The steps below show how to open it anyway. Only do this for
> files downloaded from the official releases page.

## macOS

1. Open the `.dmg` and drag **i18n-harness** into **Applications**.
2. In **Applications**, right-click (or Control-click) **i18n-harness**
   and choose **Open**.
3. macOS says it cannot verify the developer. Click **Open**.

   On recent macOS versions the dialog may only offer **Done** or **Move
   to Bin**. In that case click **Done**, then open **System Settings →
   Privacy & Security**, scroll down to the message about
   *i18n-harness*, and click **Open Anyway**. Confirm with your password.

You only need to do this once. If macOS still refuses (for example it
reports the app as "damaged"), remove the quarantine mark in **Terminal**
as a last resort:

```sh
xattr -dr com.apple.quarantine /Applications/i18n-harness.app
```

## Windows

1. Run the downloaded `-setup.exe` (or `.msi`).
2. If Microsoft Defender SmartScreen shows **Windows protected your PC**,
   click **More info**, then **Run anyway**.
3. Follow the installer. The app appears in the Start menu as
   **i18n-harness**.

## Linux

**AppImage** — make the file executable and run it:

```sh
chmod +x i18n-harness_*.AppImage
./i18n-harness_*.AppImage
```

**Debian / Ubuntu package** — install it with `apt`, which also pulls in
the system libraries the app needs (WebKitGTK):

```sh
sudo apt install ./i18n-harness_*.deb
```

The app then appears in your application menu.

## What you should see

The start screen, with **Open folder…**, **Try a sample** (until you
choose **Don’t show again** under it), a box telling you whether a model
server is running, and a link to this guide:

![Start screen](../images/start.png)

If the box says **Ollama is not running**, continue with
[Install Ollama and get a model](models.md). You can still open folders
and edit translations by hand without a model.

## Build it yourself

To build the app from source instead, follow
[Building and testing](https://github.com/VitalyVorobyev/i18n-harness/blob/main/docs/contributing/building.md)
in the project's repository.
