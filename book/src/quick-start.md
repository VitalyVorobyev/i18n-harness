# Quick start

From nothing to your first translated strings in about fifteen minutes,
most of it downloading.

## What you need

- **A computer** with macOS 11 or later, Windows 10 or 11, or a 64-bit
  Linux.
- **Memory for the model.** A model needs about as much free memory as
  its download size. 8 GB of RAM runs a small model; 16 GB is
  comfortable for the 4–9 billion parameter models that translate well.
- **Disk space:** a few hundred MB for the app and Ollama, plus 3–10 GB
  per model.
- **Your translation files** (Qt Linguist `.ts`, gettext or Lingui
  `.po`, Lingui or ICU message `.json`). If you have none at hand, the
  app has a sample.

## 1. Install the app

Download the installer for your system from the
[latest release](https://github.com/VitalyVorobyev/i18n-harness/releases/latest)
and install it:

| System | File |
|---|---|
| macOS, Apple silicon | the `.dmg` with `aarch64` in its name |
| macOS, Intel | the `.dmg` with `x64` in its name |
| Windows | the `-setup.exe` |
| Linux | the `.deb` (Debian, Ubuntu) or the `.AppImage` |

The builds are not yet signed by a known publisher, so your system warns
you the first time you open the app. On macOS, right-click the app in
**Applications** and choose **Open**; on Windows, click **More info**,
then **Run anyway**. [Install the app](getting-started/install.md) has
the details.

## 2. Install Ollama and a model

1. Download [Ollama](https://ollama.com/download) and install it. It runs
   in the background (menu bar on macOS, system tray on Windows).
2. In a terminal, download a model, for example:

   ```sh
   ollama pull gemma3:4b
   ```

   This is a 3 GB download of a general model that follows instructions
   in many languages. Any other model works too; see
   [Install Ollama and get a model](getting-started/models.md) for how
   to choose one for your languages.

## 3. Try the sample

1. Start i18n-harness. The start screen says **Ollama is running** and
   lists your model.
2. Click **Try a sample**. The app copies a small example (an
   application's German, Spanish and Chinese translation files) to the
   folder *i18n-harness sample* in your Documents and opens it. (Once
   you no longer need it, **Don’t show again** under the button hides
   the offer.)
3. Pick your model in the toolbar.
4. Click **Translate N strings**. The first string can take a minute
   while the model loads; the rest follow quickly.
5. Open the **Check** tab and click a string. Correct the suggestion if
   needed and click **Done** (<kbd>Ctrl</kbd>+<kbd>Enter</kbd>,
   <kbd>⌘</kbd>+<kbd>Enter</kbd> on macOS).
6. Click **Save** (<kbd>Ctrl</kbd>+<kbd>S</kbd>). The translations are
   written into the files; nothing else in them changes.

## 4. Your own files

Click **Close the folder** in the toolbar, then **Open folder…** and
choose the folder that holds your translation files. The app finds them
and their languages by itself.

> **Work on a copy, or under version control.** The app saves the
> translations into your files in place.

## Next

- [Your first translation](getting-started/first-translation.md) walks
  through the workspace in detail.
- The tutorials cover [Qt Linguist](tutorials/qt.md),
  [gettext PO](tutorials/po.md), [ICU JSON](tutorials/icu-json.md),
  [glossaries](tutorials/glossary.md) and
  [files pre-filled with another language](tutorials/pivot.md).
- Something not working? See [Troubleshooting](reference/troubleshooting.md).
