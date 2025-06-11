RFLAGS="-C link-arg=-s"

build: lint satoshi-bridge nbtc mock-chain-signatures mock-btc-light-client mock-dapp

lint:
	@cargo fmt --all
	@cargo clippy --fix --allow-dirty --allow-staged

satoshi-bridge: contracts/satoshi-bridge
	$(call local_build_wasm,satoshi-bridge,satoshi_bridge)

nbtc: contracts/nbtc
	$(call local_build_wasm,nbtc,nbtc)

mock-dapp: contracts/mock-dapp
	$(call local_build_wasm,mock-dapp,mock_dapp)

mock-chain-signatures: contracts/mock-chain-signatures
	$(call local_build_wasm,mock-chain-signatures,mock_chain_signatures)

mock-btc-light-client: contracts/mock-btc-light-client
	$(call local_build_wasm,mock-btc-light-client,mock_btc_light_client)

count:
	@tokei ./contracts/satoshi-bridge/src/ --files --exclude unit
	@tokei ./contracts/nbtc/src/ --files

release:
	$(call build_release_wasm,satoshi-bridge,satoshi_bridge)
	$(call build_release_wasm,nbtc,nbtc)

clean:
	cargo clean
	rm -rf res/

define local_build_wasm
	$(eval PACKAGE_NAME := $(1))
	$(eval WASM_NAME := $(2))

	@mkdir -p res
	@rustup target add wasm32-unknown-unknown
	@cargo near build non-reproducible-wasm --manifest-path ./contracts/${PACKAGE_NAME}/Cargo.toml --locked --no-abi
	@cp target/near/${WASM_NAME}/$(WASM_NAME).wasm ./res/$(WASM_NAME).wasm
endef

define build_release_wasm
	$(eval PACKAGE_NAME := $(1))
	$(eval WASM_NAME := $(2))

	@mkdir -p res
	@rustup target add wasm32-unknown-unknown
	@cargo near build reproducible-wasm --manifest-path ./contracts/${PACKAGE_NAME}/Cargo.toml
	@cp target/near/${WASM_NAME}/$(WASM_NAME).wasm ./res/$(WASM_NAME)_release.wasm
endef