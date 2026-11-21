# Called by the root Makefile after toolchain selection and the build lock.
SHELL := /bin/bash
.NOTPARALLEL:
.DELETE_ON_ERROR:
ANGLE := $(VSF_ANGLE_BUILD)
PLACEBO := $(VSF_PLACEBO_BUILD)
MPV := $(VSF_MPV_BUILD)
FFMPEG := $(VSF_FFMPEG_BUILD)
DOVI := $(VSF_DOVI_BUILD)
MPV_SDK := $(VSF_MPV_SDK_BUILD)
UPSTREAM := $(VSF_MPV_BUILD)-upstream
PLATFORM := $(VSF_NATIVE_PLATFORM)
PROFILE := $(VSF_NATIVE_PROFILE)
JOBS := $(VSF_NATIVE_JOBS)
# Root PYTHON selects the venv creator; recursive command-line variables must
# not override the interpreter containing the pinned native tools.
override PYTHON := $(VSF_NATIVE_PYTHON)
OUT := target/native/$(PLATFORM)/$(PROFILE)
TARGET := $(CARGO_TARGET_DIR)/$(PROFILE)
STAGE := bash scripts/native-stage.sh
PUBLISH := bash scripts/native-publish.sh
TAURI_PROFILE := $(if $(filter debug,$(PROFILE)),--debug,)
CARGO_PROFILE := $(if $(filter release,$(PROFILE)),--release,)
FEATURES := media-prototype,bundled-tools
BUNDLE := --no-bundle

ifeq ($(PLATFORM),macos)
FEATURES := macos-angle,bundled-tools
BUNDLE := --bundles app
export VSF_ANGLE_SDK := $(abspath $(ANGLE)/sdk)
export PKG_CONFIG_PATH := $(abspath $(MPV)/build/meson-uninstalled):$(abspath $(PLACEBO)/sdk/lib/pkgconfig):$(PKG_CONFIG_PATH)
NATIVE := $(ANGLE)/.complete $(MPV)/.complete
else ifeq ($(PLATFORM),linux)
export PKG_CONFIG_PATH := $(abspath $(MPV)/build/meson-uninstalled):$(abspath $(PLACEBO)/sdk/lib/pkgconfig):$(abspath $(FFMPEG)/sdk/lib/pkgconfig):$(abspath $(DOVI)/sdk/lib/pkgconfig):$(PKG_CONFIG_PATH)
export LD_LIBRARY_PATH := $(abspath $(MPV)/build):$(abspath $(PLACEBO)/sdk/lib):$(abspath $(FFMPEG)/sdk/lib):$(abspath $(DOVI)/sdk/lib)
NATIVE := $(MPV)/.complete
else ifeq ($(PLATFORM),windows)
export VSF_MPV_SDK := $(shell cygpath -m '$(abspath $(MPV_SDK))')
export PKG_CONFIG_PATH := $(abspath $(FFMPEG)/sdk/lib/pkgconfig)
export PATH := $(abspath $(FFMPEG)/sdk/bin):$(PATH)
MPV_COMPILER := env CC=clang CXX=clang++
NATIVE := $(MPV_SDK)/.complete
else
$(error Run the root Makefile, not build/native.mk directly)
endif

.PHONY: build build-install app package install paths
paths:
	@:

build: package
build-install: build install

include build/mpv.mk
ifeq ($(PLATFORM),macos)
include build/angle.mk build/placebo.mk
else ifeq ($(PLATFORM),linux)
include build/ffmpeg.mk build/placebo.mk
else
include build/ffmpeg.mk build/mpv-sdk.mk
endif

.PHONY: native
native: $(NATIVE)

NPM_STAMP := node_modules/.vsf-npm-stamp
$(NPM_STAMP): package.json package-lock.json .nvmrc $(wildcard .npmrc)
	npm ci
	touch "$@"

ifeq ($(CI),true)
.PHONY: force-npm-ci
$(NPM_STAMP): force-npm-ci
endif

app: $(NATIVE) $(NPM_STAMP)
	@mkdir -p "$(OUT)"
	@case "$(PLATFORM)" in macos) runtime=("$(MPV)/build/libmpv.2.dylib");; linux) runtime=("$(MPV)/build/libmpv.so");; windows) runtime=("$(MPV)/build/"*mpv*.dll);; esac; \
	  test "$${#runtime[@]}" = 1 && $(PYTHON) scripts/native-preflight.py --feature $(if $(filter macos,$(PLATFORM)),macos-angle,media-prototype) --mpv-runtime "$${runtime[0]}" --mpv-build-receipt "$(MPV)/build-receipt.json" --output "$(OUT)/preflight-$$(date +%s)-$$$$.json"
	npm run tauri -- build $(TAURI_PROFILE) $(BUNDLE) --features $(FEATURES) -- --locked
	cargo build --locked $(CARGO_PROFILE) -p vsf-cli --features bundled-tools

ifeq ($(PLATFORM),macos)
package: app
	$(PUBLISH) "$(OUT)/package" $(PYTHON) scripts/package-macos.py --app "$(TARGET)/bundle/macos/VisualSubForge.app" --vsf "$(TARGET)/vsf" --ffmpeg "$$(command -v ffmpeg)" --ffprobe "$$(command -v ffprobe)" --library-dir "$(MPV)/build" --library-dir "$(ANGLE)/sdk/out/Metal" --library-dir "$(PLACEBO)/sdk/lib" --output '{output}/VisualSubForge.app'
else ifeq ($(PLATFORM),linux)
package: app
	$(PUBLISH) "$(OUT)/package" $(PYTHON) scripts/package-linux.py --visualsubforge "$(TARGET)/visualsubforge" --vsf "$(TARGET)/vsf" --build-paths "target/native/linux/build-paths.json" --output '{output}'
else
package: app
	$(PUBLISH) "$(OUT)/package" $(PYTHON) scripts/package-windows.py --visualsubforge "$(TARGET)/visualsubforge.exe" --vsf "$(TARGET)/vsf.exe" --ffmpeg "$(FFMPEG)/sdk/bin/ffmpeg.exe" --ffprobe "$(FFMPEG)/sdk/bin/ffprobe.exe" --library-dir "$(MPV)/build" --library-dir "$(FFMPEG)/sdk/bin" --library-dir "$$(cygpath -m /clang64/bin)" --output '{output}'
endif

install:
	bash scripts/native-install.sh "$(PLATFORM)" "$(OUT)/package"
