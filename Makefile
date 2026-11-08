# Run inside `nix develop`. Plugins land in build/.
#
#   make clap      build/CONTRA.clap
#   make vst3      build/CONTRA.vst3
#   make           both
#   make install   both, copied to ~/.clap and ~/.vst3 (where DAWs look)
#   make test      the test suite
#   make clean

SO := target/release/libcontra_plugin.so

.PHONY: all clap vst3 install test clean so

all: clap vst3

so:
	@test -n "$$CONTRA_RPATH" || { echo "Run this inside 'nix develop'."; exit 1; }
	cargo build --release -p contra-plugin

clap: so
	mkdir -p build
	install -m755 $(SO) build/CONTRA.clap
	patchelf --add-rpath "$$CONTRA_RPATH" build/CONTRA.clap
	@echo "Built build/CONTRA.clap"

vst3: so
	mkdir -p build/CONTRA.vst3/Contents/x86_64-linux
	install -m755 $(SO) build/CONTRA.vst3/Contents/x86_64-linux/CONTRA.so
	patchelf --add-rpath "$$CONTRA_RPATH" build/CONTRA.vst3/Contents/x86_64-linux/CONTRA.so
	@echo "Built build/CONTRA.vst3"

install: all
	mkdir -p $(HOME)/.clap $(HOME)/.vst3
	install -m755 build/CONTRA.clap $(HOME)/.clap/CONTRA.clap
	rm -rf $(HOME)/.vst3/CONTRA.vst3
	cp -r build/CONTRA.vst3 $(HOME)/.vst3/CONTRA.vst3
	@echo "Installed to ~/.clap and ~/.vst3"

test:
	mkdir -p target/gpu-receipts
	VK_DRIVER_FILES=$$CONTRA_SOFTWARE_VULKAN CONTRA_GPU_RECEIPT_DIR=$(CURDIR)/target/gpu-receipts cargo test --workspace

clean:
	rm -rf build
	cargo clean
