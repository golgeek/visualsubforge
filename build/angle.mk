$(ANGLE)/.complete:
	$(STAGE) "$(ANGLE)" $(PYTHON) scripts/build-angle-macos.py --output '{output}' --jobs $(JOBS)

