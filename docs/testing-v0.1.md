# Try VisualSubForge in 15 minutes

VisualSubForge is an early subtitle editor. This test uses short, synthetic clips. Please report what happened on your machine, including a clean run. You can stop at any step and mark the rest **Not tried**.

## Download and install

Open the [v0.1.0 test release](https://github.com/golgeek/visualsubforge/releases/tag/v0.1.0). Download **VisualSubForge-0.1.0-test-media.zip** (about 46 MiB) and extract it into a writable folder. Keep a separate folder for your edited files. The release includes a SHA-256 checksum for each download and the pack includes its own checksums and provenance.

| Your computer | What to do |
| --- | --- |
| Apple Silicon Mac, macOS 14 or newer | Unzip **VisualSubForge_0.1.0_aarch64_macos.zip**, drag the app to Applications and open it. |
| Intel Mac, macOS 15 or newer | Use **VisualSubForge_0.1.0_x86_64_macos.zip** and the same steps. |
| Windows x64 | Run **VisualSubForge_0.1.0_x64-setup.exe**. This early build is unsigned. If SmartScreen shows “Windows protected your PC”, choose **More info → Run anyway** after verifying the download. A later version will be signed. For a portable installation, extract the entire Windows ZIP and open **visualsubforge.exe**. Both use WebView2. Record your Windows version. |
| Ubuntu 24.04 amd64 | In your download folder run `sudo apt install ./VisualSubForge_0.1.0_amd64_ubuntu24.04.deb`, then open VisualSubForge from Applications. This version supports Ubuntu 24.04 only. |

## The test

1. **Open the sample project.** Choose **File → Open Project…** and open `sample.vsfproject`. Two subtitle cues appear, including “VSF preview: Café”.
2. **Open the video.** Choose **File → Open Media…** and select `h264-1080p.mp4`. The test pattern should appear alongside the cues.
3. **Play and seek.** Press Play, listen for a steady tone, then seek from before six seconds to after it. The image changes colour and playback resumes at the chosen time.
4. **Preview subtitles.** Pause around two seconds. The first cue appears on the video. Around seven seconds the second cue appears.
5. **Use the waveform.** Wait for the waveform, then click an empty part between the two cues. The playhead and picture should follow.
6. **Scan for shot changes.** Choose **Review → Scan Scene Changes**. Enable **Scene changes** if needed. Look for a marker near the colour change at six seconds.
7. **Edit one cue.** Select the first cue and change “Café” to “Coffee”. Pause typing, then click another cue. The list should retain the edit.
8. **Split a cue.** Select the first cue, place the playhead inside it around two seconds and choose **Cue → Split at Playhead**. Two adjacent cues should replace it.
9. **Merge the parts.** Select the earlier part and choose **Cue → Merge with Next**. The two parts should become one cue.
10. **Undo.** Choose **Edit → Undo**. The split cues should return.
11. **Save and reopen.** Choose **File → Save Subtitles As…** and save a new SRT outside the sample folder. Quit and reopen VSF, then open your saved SRT. Check the edited text and cue times.
12. **Switch audio tracks.** Open `vfr-offset.mkv`. In **Audio track**, try each of the two tracks from the beginning. Listen for a short impulse around 1.25 seconds on one and 2 seconds on the other.
13. **Resynchronize.** Open `resync/offset.mkv` and `resync/source.srt`. Choose **Review → Resynchronize Subtitles…** and select `resync/source.mkv` as the source video. Choose **Analyze videos** and review the proposed timings. Compare them to `resync/offset.expected.srt` (the first cue starts at six seconds). Choose **Close** to return to the editor.
14. **Export a copy.** With an SRT open, choose **File → Export As…**, choose ASS, then **Review conversion → Choose new output file…**. When **Export complete** appears, close the dialog and open that file and check the text and times. The pack also includes `subtitles/styles.ass` to try.
15. **Check for updates.** Choose **Help → Check for Updates…**. Expect a current-version message, a newer release link, or an explanation if GitHub cannot be reached. Automatic checks are optional in **Settings → Advanced**.

## Send your results

Open the [test report form](https://github.com/golgeek/visualsubforge/issues/new?template=test-report.yml). Choose **Pass**, **Fail** or **Not tried** for each step. Include your audio output, install method and any security prompt.

- **Mac:** Apple menu → About This Mac gives the macOS version and chip. System Information → Graphics/Displays lists the GPU. Record the selected size in System Settings → Displays.
- **Windows:** Run `winver` for the OS build. Settings → System → About shows the system type. Task Manager → Performance → GPU or `dxdiag` shows graphics details. Settings → System → Display shows Scale.
- **Ubuntu:** Settings → System → About shows OS and graphics information. `uname -m` gives the architecture and `echo "$XDG_SESSION_TYPE"` gives Wayland or X11. Record the scale shown in Settings → Displays and the GPU driver if known.

VSF has no automatic crash upload or general application log file. Copy the exact on-screen error. For a crash, macOS Console → Crash Reports, Windows Event Viewer → Windows Logs → Application, or the Linux journal may contain an entry at the time of the failure. Please review any excerpt for names and paths before attaching it.

Please use the sample files for screenshots and attachments. For private or copyrighted media, describe the container, codecs, resolution and duration, and keep the media and subtitles on your machine.
