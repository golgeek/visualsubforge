$(SHADERS)/.complete:
	$(STAGE) "$(SHADERS)" sh scripts/build-linux-shader-stack.sh '{output}' $(JOBS)
