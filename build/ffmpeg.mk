ifneq ($(PLATFORM),windows)
$(FFMPEG)/.complete:
	$(STAGE) "$(FFMPEG)" sh scripts/build-ffmpeg.sh '{output}' $(JOBS)

ifeq ($(PLATFORM),linux)
$(DOVI)/.complete:
	$(STAGE) "$(DOVI)" sh scripts/build-linux-dovi.sh '{output}'
endif
else
$(FFMPEG)/.complete:
	$(STAGE) "$(FFMPEG)" bash scripts/build-ffmpeg-windows.sh '{output}' $(JOBS)

endif
