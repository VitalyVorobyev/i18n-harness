<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<TS version="2.1" language="zh_Hans" sourcelanguage="en">
<context>
    <name>MainWindow</name>
    <message>
        <location filename="src/mainwindow.cpp" line="42"/>
        <source>Hello</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="58"/>
        <source>Open %1 from %2</source>
        <extracomment>Status bar message when opening a file from a recent project.</extracomment>
        <translation type="unfinished"></translation>
    </message>
    <message numerus="yes">
        <location filename="src/mainwindow.cpp" line="75"/>
        <source>%n unread message(s)</source>
        <translation type="unfinished">
            <numerusform></numerusform>
        </translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="92"/>
        <source>&amp;File</source>
        <comment>Top-level menu</comment>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="108"/>
        <source>Save</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="115"/>
        <source>Cancel</source>
        <translation type="unfinished"></translation>
    </message>
</context>
<context>
    <name>SettingsDialog</name>
    <message>
        <location filename="src/settings.cpp" line="20"/>
        <source>Theme</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="src/settings.cpp" line="28"/>
        <source>Light</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="src/settings.cpp" line="29"/>
        <source>Dark</source>
        <translation type="unfinished"></translation>
    </message>
</context>
<context>
    <name>welcome_screen</name>
    <message>
        <location filename="src/welcome.cpp" line="10"/>
        <source>Welcome to the localization workbench. Open any project to see its translation status across every supported locale. Use the matrix view to compare translations side by side, or focus on a single locale to power through untranslated units one at a time. The model suggests translations; you review, edit, and accept. Glossary terms keep your terminology consistent across the project.</source>
        <extracomment>Onboarding paragraph shown on the welcome screen when no project is open.</extracomment>
        <translation type="unfinished"></translation>
    </message>
</context>
<context>
    <name>release_notes</name>
    <message>
        <location filename="src/releasenotes.cpp" line="10"/>
        <source>Version 2.0 introduces a redesigned translation matrix that loads projects with hundreds of locales in under a second. Columns are now sortable by completion rate, and the active cell scrolls into view automatically.

The crash that occurred when switching locales while a background translation was still running has been fixed. Progress state is now saved before the locale switch completes.

Undo history is currently limited to the active session and is not persisted to disk. Restarting the application clears all undo steps. This will be addressed in a future release.</source>
        <extracomment>Release notes shown in the About dialog. Three paragraphs: new feature, bug fix, known issue.</extracomment>
        <translation type="unfinished"></translation>
    </message>
</context>
<context>
    <name>download_progress</name>
    <message numerus="yes">
        <location filename="src/download.cpp" line="10"/>
        <source>%n file pending download from the remote archive — keep this window open until the download completes or the transfer will be cancelled.</source>
        <extracomment>Shown in the download status bar. %n is the number of files remaining.</extracomment>
        <translation type="unfinished">
            <numerusform></numerusform>
        </translation>
    </message>
</context>
</TS>
