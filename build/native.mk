# Called by the root Makefile after toolchain selection and the build lock.
SHELL := /bin/bash
.NOTPARALLEL:
.DELETE_ON_ERROR:
CACHE := $(VSF_NATIVE_CACHE)
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
export VSF_ANGLE_SDK := $(abspath $(CACHE)/angle/sdk)
export PKG_CONFIG_PATH := $(abspath $(CACHE)/mpv/build/meson-uninstalled):$(abspath $(CACHE)/placebo/sdk/lib/pkgconfig):$(PKG_CONFIG_PATH)
NATIVE := $(CACHE)/angle/.complete $(CACHE)/mpv/.complete
else ifeq ($(PLATFORM),linux)
export PKG_CONFIG_PATH := $(abspath $(CACHE)/mpv/build/meson-uninstalled):$(abspath $(CACHE)/placebo/sdk/lib/pkgconfig):$(PKG_CONFIG_PATH)
export LD_LIBRARY_PATH := $(abspath $(CACHE)/mpv/build):$(abspath $(CACHE)/placebo/sdk/lib)
NATIVE := $(CACHE)/mpv/.complete
else ifeq ($(PLATFORM),windows)
export VSF_MPV_SDK := $(shell cygpath -m '$(abspath $(CACHE)/mpv-sdk)')
export PKG_CONFIG_PATH := $(abspath $(CACHE)/ffmpeg/sdk/lib/pkgconfig)
export PATH := $(abspath $(CACHE)/ffmpeg/sdk/bin):$(PATH)
MPV_COMPILER := env CC=clang CXX=clang++
NATIVE := $(CACHE)/mpv-sdk/.complete
else
$(error Run the root Makefile, not build/native.mk directly)
endif

.PHONY: build build-install app package install
build: package
build-install: build install

$(CACHE)/angle/.complete:
	$(STAGE) "$(CACHE)/angle" $(PYTHON) scripts/build-angle-macos.py --output '{output}' --jobs $(JOBS)

$(CACHE)/placebo/.complete:
	$(STAGE) "$(CACHE)/placebo" $(PYTHON) scripts/build-placebo.py --output '{output}' --jobs $(JOBS)

$(CACHE)/upstream/.complete:
	$(STAGE) "$(CACHE)/upstream" git clone https://github.com/mpv-player/mpv.git '{output}'

$(CACHE)/ffmpeg/.complete:
	$(STAGE) "$(CACHE)/ffmpeg" bash scripts/build-ffmpeg-windows.sh '{output}'

$(CACHE)/mpv/.complete: $(CACHE)/upstream/.complete $(if $(filter windows,$(PLATFORM)),$(CACHE)/ffmpeg/.complete,$(CACHE)/placebo/.complete)
	$(STAGE) "$(CACHE)/mpv" $(MPV_COMPILER) $(PYTHON) scripts/check-mpv-build.py --source "$(CACHE)/upstream" --output '{output}' --variant candidates --jobs $(JOBS)

$(CACHE)/mpv-sdk/.complete: $(CACHE)/mpv/.complete
	$(STAGE) "$(CACHE)/mpv-sdk" $(PYTHON) scripts/prepare-windows-sdk.py --mpv "$(CACHE)/mpv" --output '{output}'

app: $(NATIVE)
	@mkdir -p "$(OUT)"
	@case "$(PLATFORM)" in macos) runtime=("$(CACHE)/mpv/build/libmpv.2.dylib");; linux) runtime=("$(CACHE)/mpv/build/libmpv.so");; windows) runtime=("$(CACHE)/mpv/build/"*mpv*.dll);; esac; \
	  test "$${#runtime[@]}" = 1 && $(PYTHON) scripts/native-preflight.py --feature $(if $(filter macos,$(PLATFORM)),macos-angle,media-prototype) --mpv-runtime "$${runtime[0]}" --mpv-build-receipt "$(CACHE)/mpv/build-receipt.json" --output "$(OUT)/preflight-$$(date +%s)-$$$$.json"
	npm ci
	npm run tauri -- build $(TAURI_PROFILE) $(BUNDLE) --features $(FEATURES) -- --locked
	cargo build --locked $(CARGO_PROFILE) -p vsf-cli --features bundled-tools

ifeq ($(PLATFORM),macos)
package: app
	$(PUBLISH) "$(OUT)/package" $(PYTHON) scripts/package-macos.py --app "$(TARGET)/bundle/macos/VisualSubForge.app" --vsf "$(TARGET)/vsf" --ffmpeg "$$(command -v ffmpeg)" --ffprobe "$$(command -v ffprobe)" --library-dir "$(CACHE)/mpv/build" --library-dir "$(CACHE)/angle/sdk/out/Metal" --library-dir "$(CACHE)/placebo/sdk/lib" --output '{output}/VisualSubForge.app'
else ifeq ($(PLATFORM),linux)
package: app
	$(PUBLISH) "$(OUT)/package" $(PYTHON) scripts/package-linux.py --visualsubforge "$(TARGET)/visualsubforge" --vsf "$(TARGET)/vsf" --ffmpeg /usr/bin/ffmpeg --ffprobe /usr/bin/ffprobe --library-dir "$(CACHE)/mpv/build" --library-dir "$(CACHE)/placebo/sdk/lib" --native-source "mpv=$(CACHE)/mpv/source.tar.gz" --native-source "libplacebo=$(CACHE)/placebo/source.tar.gz" --native-notices "mpv=$(CACHE)/mpv/prepared/source" --native-notices "libplacebo=$(CACHE)/placebo/source" --placebo-build "$(CACHE)/placebo" --output '{output}'
else
package: app
	$(PUBLISH) "$(OUT)/package" $(PYTHON) scripts/package-windows.py --visualsubforge "$(TARGET)/visualsubforge.exe" --vsf "$(TARGET)/vsf.exe" --ffmpeg "$(CACHE)/ffmpeg/sdk/bin/ffmpeg.exe" --ffprobe "$(CACHE)/ffmpeg/sdk/bin/ffprobe.exe" --library-dir "$(CACHE)/mpv/build" --library-dir "$(CACHE)/ffmpeg/sdk/bin" --library-dir "$$(cygpath -m /clang64/bin)" --output '{output}'
endif

install:
	bash scripts/native-install.sh "$(PLATFORM)" "$(OUT)/package"
