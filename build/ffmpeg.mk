$(FFMPEG)/.complete:
	$(STAGE) "$(FFMPEG)" bash scripts/build-ffmpeg-windows.sh '{output}' $(JOBS)

