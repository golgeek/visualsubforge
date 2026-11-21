$(PLACEBO)/.complete: $(FFMPEG)/.complete $(if $(filter linux,$(PLATFORM)), $(DOVI)/.complete $(SHADERS)/.complete)
	$(STAGE) "$(PLACEBO)" $(PYTHON) scripts/build-placebo.py --output '{output}' --jobs $(JOBS)

