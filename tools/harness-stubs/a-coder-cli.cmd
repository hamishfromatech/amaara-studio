@echo off
rem Windows shim: the adapter resolves a-coder-cli via `where`, which finds
rem this .cmd; the adapter runs it through `cmd /c` (see aacoder.rs start()).
node "%~dp0a-coder-cli-stub.mjs" %*