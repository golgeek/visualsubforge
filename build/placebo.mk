$(PLACEBO)/.complete: $(if $(filter linux,$(PLATFORM)),$(FFMPEG)/.complete $(DOVI)/.complete)
	$(STAGE) "$(PLACEBO)" $(PYTHON) scripts/build-placebo.py --output '{output}' --jobs $(JOBS)

