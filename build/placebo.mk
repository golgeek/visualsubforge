$(PLACEBO)/.complete:
	$(STAGE) "$(PLACEBO)" $(PYTHON) scripts/build-placebo.py --output '{output}' --jobs $(JOBS)

