"""Decode the shared composition probes' min/max audio bins with host FFmpeg."""
import array
import subprocess
import sys


def decoded_peaks(media):
    pcm = subprocess.check_output([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-i", str(media),
        "-vn", "-t", "6.01", "-ac", "1", "-ar", "48000", "-f", "f32le", "-",
    ])
    samples = array.array("f", pcm)
    if sys.byteorder != "little":
        samples.byteswap()
    peaks = [[round(min(samples[i:i+480]), 5), round(max(samples[i:i+480]), 5)]
             for i in range(0, len(samples), 480)]
    return peaks, len(samples)
