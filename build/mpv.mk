$(UPSTREAM)/.complete:
	$(STAGE) "$(UPSTREAM)" git clone https://github.com/mpv-player/mpv.git '{output}'

$(MPV)/.complete: $(UPSTREAM)/.complete $(if $(filter windows,$(PLATFORM)),$(FFMPEG)/.complete,$(PLACEBO)/.complete)
	$(STAGE) "$(MPV)" $(MPV_COMPILER) $(PYTHON) scripts/check-mpv-build.py --source "$(UPSTREAM)" --output '{output}' --variant candidates --jobs $(JOBS)

