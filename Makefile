MAKEFILE_DIR :=  $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST))))
#INT_OPTIONS = -D warnings -D clippy::pedantic -A clippy::must_use_candidate -A clippy::used_underscore_binding -A clippy::needless_range_loop //TODO: enable it later
BRIDGE_MANIFEST := $(MAKEFILE_DIR)/contracts/satoshi-bridge/Cargo.toml

RFLAGS="-C link-arg=-s"

FEATURES = bitcoin zcash dash

release: $(addprefix build-,$(FEATURES))
	$(call build_release_wasm,nbtc,nbtc)

build-local: $(addprefix build-local-,$(FEATURES)) nbtc mock-chain-signatures mock-btc-light-client mock-dapp

lint: $(addprefix clippy-,$(FEATURES)) $(addprefix fmt-,$(FEATURES))
	@cargo fmt --all
	@cargo clippy -- $(LINT_OPTIONS)

test: build-local $(addprefix test-,$(FEATURES))

$(foreach feature,$(FEATURES), \
	$(eval build-$(feature): ; \
		cargo near build reproducible-wasm --variant "$(feature)" --manifest-path $(BRIDGE_MANIFEST) && \
		mkdir -p res && mv ./target/near/satoshi_bridge/satoshi_bridge.wasm ./res/$(feature)_bridge_release.wasm \
	) \
)

$(foreach feature,$(FEATURES), \
	$(eval build-local-$(feature): ; \
		cargo near build non-reproducible-wasm --features "$(feature)" --manifest-path $(BRIDGE_MANIFEST) --no-abi && \
		mkdir -p res && mv ./target/near/satoshi_bridge/satoshi_bridge.wasm ./res/$(feature)_bridge.wasm \
	) \
)

$(foreach feature,$(FEATURES), \
	$(eval clippy-$(feature): ; cargo clippy --no-default-features --features "$(feature)" --manifest-path $(BRIDGE_MANIFEST) -- $(LINT_OPTIONS)) \
)

$(foreach feature,$(FEATURES), \
	$(eval fmt-$(feature): ; cargo fmt --all --check --manifest-path $(BRIDGE_MANIFEST)) \
)

$(foreach feature,$(FEATURES), \
	$(eval test-$(feature): ; cargo test --no-default-features --features "$(feature)" --manifest-path $(BRIDGE_MANIFEST)) \
)


mock-dapp: contracts/mock-dapp
	$(call local_build_wasm,mock-dapp,mock_dapp)

mock-chain-signatures: contracts/mock-chain-signatures
	$(call local_build_wasm,mock-chain-signatures,mock_chain_signatures)

mock-btc-light-client: contracts/mock-btc-light-client
	$(call local_build_wasm,mock-btc-light-client,mock_btc_light_client)

nbtc: contracts/nbtc
	$(call local_build_wasm,nbtc,nbtc)
	
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

define build_release_zcash_wasm
	@mkdir -p res
	@rustup target add wasm32-unknown-unknown
	@cargo near build reproducible-wasm --manifest-path ./contracts/satoshi-bridge/Cargo.toml --variant zcash
	@cp target/near/satoshi_bridge/satoshi_bridge.wasm ./res/zcash_connector_release.wasm
endef

define local_build_zcash_wasm
    @mkdir -p res
    @rustup target add wasm32-unknown-unknown
    @cargo near build non-reproducible-wasm --manifest-path ./contracts/satoshi-bridge/Cargo.toml --locked --no-abi --no-default-features --features zcash
    @cp target/near/satoshi_bridge/satoshi_bridge.wasm ./res/zcash.wasm
endef
