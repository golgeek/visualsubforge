ifeq ($(PLATFORM),linux)
$(FFMPEG)/.complete:
	$(STAGE) "$(FFMPEG)" sh scripts/build-ffmpeg.sh '{output}' $(JOBS)

$(DOVI)/.complete:
	$(STAGE) "$(DOVI)" sh scripts/build-linux-dovi.sh '{output}'
else
$(FFMPEG)/.complete:
	$(STAGE) "$(FFMPEG)" bash scripts/build-ffmpeg-windows.sh '{output}' $(JOBS)

endif
