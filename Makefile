# GNU Make 3.81+; Windows uses the MSYS2 CLANG64 shell.
.DEFAULT_GOAL := build
.NOTPARALLEL:
SHELL := /bin/bash
PROFILE := $(if $(filter debug,$(MAKECMDGOALS)),debug,release)
MAKE_COMMAND := $(MAKE)
JOBS ?= 6
PYTHON ?= python3

.PHONY: build debug install help
build debug:
	@bash scripts/native-env.sh "$(MAKE_COMMAND)" "$(PROFILE)" "$(JOBS)" "$(PYTHON)" build

# With two goals, the build above has already completed. install alone builds first.
install:
	@bash scripts/native-env.sh "$(MAKE_COMMAND)" "$(PROFILE)" "$(JOBS)" "$(PYTHON)" $(if $(filter build debug,$(MAKECMDGOALS)),install,build-install)

help:
	@printf '%s\n' 'make build          Native release app (default)' 'make debug          Native debug app, with symbols and webview devtools' 'make install        Build and install release app' 'make debug install  Build and install debug app' 'JOBS=6 PYTHON=python3; see docs/build.md for prerequisites and output paths.'
