@echo off
echo Building FastDupFinder...
cargo build --release
echo Done: target\release\fastdupfinder.exe
copy target\release\fastdupfinder.exe . 2>nul
