# Windows FFmpeg build correction

The Windows package builds checksum-pinned FFmpeg 8.1.2 with the explicit recipe
in `scripts/build-ffmpeg-windows.sh`. It retains built-in filters and codecs.

The one-line LGPL-2.1-or-later patch includes the standard header declaring
`std::system_error`, already used by WinRT capture. Hosted run 38192280209
failed compiling that handler with CLANG64 libc++; relying on a transitive
include is not portable. The patch changes no filter behavior. Its context must
apply without fuzz; re-evaluate it on every FFmpeg upgrade and remove it when
upstream includes the declaration directly.

The recipe also renames `VERSION` to `VERSION.txt`, as the matching MSYS2 source
recipe does, to prevent Windows resolving C++20 `<version>` to FFmpeg's file.
Both changes must accompany the original source archive in redistribution.
Compilation, packaged analysis and native playback remain separate checks.
