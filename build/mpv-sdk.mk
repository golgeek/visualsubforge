$(MPV_SDK)/.complete: $(MPV)/.complete
	$(STAGE) "$(MPV_SDK)" $(PYTHON) scripts/prepare-windows-sdk.py --mpv "$(MPV)" --output '{output}'

