<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<!-- Hand-crafted showcase fixture for the M0 round-trip contract.
     Covers: simple message, %1/%2 placeholders, %n plural with two numerusforms,
     accelerator (&File), <comment> and <extracomment>, vanished state,
     obsolete state, significant whitespace and indentation, CDATA,
     and XML entity escapes (&amp;, &lt;). -->
<TS version="2.1" language="de_DE" sourcelanguage="en">
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
            <numerusform></numerusform>
        </translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="91"/>
        <source>&amp;File</source>
        <comment>menu</comment>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="92"/>
        <source>Click &lt;b&gt;Save&lt;/b&gt; to continue.</source>
        <translation type="unfinished"></translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="93"/>
        <source>Legacy import path</source>
        <translation type="vanished">Veralteter Importpfad</translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="94"/>
        <source>Old splash text</source>
        <translation type="obsolete">Alter Startbildschirmtext</translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="120"/>
        <source>Verbatim block</source>
        <translation type="unfinished"><![CDATA[A & B < C]]></translation>
    </message>
</context>
<context>
    <name>SettingsDialog</name>
    <message>
        <location filename="src/settings.cpp" line="14"/>
        <source>Restart required</source>
        <translation type="unfinished"></translation>
    </message>
</context>
</TS>
