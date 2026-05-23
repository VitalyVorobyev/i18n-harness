<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE TS>
<!-- Round-trip fixture for the zh_Hans locale (M3 add-locale).
     Plural arity 1 (just `other`) — Mandarin has no singular/plural
     distinction. Han script activates the gate's CJK punctuation rule.
     Pre-filled with deliberately mixed punctuation (ASCII comma vs full
     width 。 ， ！ ：) so the gate's CjkPunctuationTolerated soft check
     has something to flag when run against this fixture. -->
<TS version="2.1" language="zh_Hans" sourcelanguage="en">
<context>
    <name>MainWindow</name>
    <message>
        <location filename="src/mainwindow.cpp" line="8"/>
        <source>Hello, world.</source>
        <translation>你好，世界。</translation>
    </message>
    <message>
        <location filename="src/mainwindow.cpp" line="42"/>
        <source>Open %1</source>
        <translation>打开 %1</translation>
    </message>
    <message numerus="yes">
        <location filename="src/mainwindow.cpp" line="75"/>
        <source>%n unread message(s)</source>
        <translation>
            <numerusform>%n 条未读消息</numerusform>
        </translation>
    </message>
</context>
</TS>
