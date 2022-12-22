DOCKER ?= docker
SYNTHETIC_NETWORK ?= 10.77.0.0/16
CONTAINER_NAME_INTERACTIVE ?= syntheticnet-interactive
CONTAINER_NAME_CHROME ?= syntheticnet-chrome
TESTHOST ?= ''

help: # Print this help message
	$(info SYNTHETIC_NETWORK ?= ${SYNTHETIC_NETWORK})
	$(info CONTAINER_NAME_INTERACTIVE ?= ${CONTAINER_NAME_INTERACTIVE})
	$(info CONTAINER_NAME_CHROME ?= ${CONTAINER_NAME_CHROME})
	$(info TESTHOST ?= <hostname>:<address> (add /etc/hosts entry to container))
	@grep '^[^#[:space:]\\.].*:' Makefile

image: # Build Docker image: syntheticnet
	$(DOCKER) build -t syntheticnet .

image-vnc: # Build Docker image: syntheticnet:vnc
	$(DOCKER) build -t syntheticnet:vnc --build-arg VNC=true .

image-chrome: image-vnc # Build Docker image: syntheticnet:chrome
	$(DOCKER) build -t syntheticnet:chrome synth-chrome/

rush: # Build rush
	cd rush && cargo clean && cargo build --release

minimal: rush # Build Docker image: syntheticnet:minimal
	$(DOCKER) build -t syntheticnet:minimal -f Dockerfile.minimal .

run-interactive: image synthetic-network # Debug syntheticnet container. Prereq: create-synthetic-network
	$(DOCKER) rm $(CONTAINER_NAME_INTERACTIVE) || true
	$(DOCKER) create --privileged \
		--env SYNTHETIC_NETWORK=$(SYNTHETIC_NETWORK) \
		--publish 3000:80 \
		--name $(CONTAINER_NAME_INTERACTIVE) \
		--tty --interactive --env ENTRY=bash \
		$(shell [ -n $(TESTHOST) ] && echo '--add-host="$(TESTHOST)"') \
		syntheticnet
	$(DOCKER) network connect synthetic-network $(CONTAINER_NAME_INTERACTIVE)
	@echo
	@echo "🎛 Synthetic network GUI will listen on http://localhost:3000"
	@echo
	$(DOCKER) start --attach --interactive $(CONTAINER_NAME_INTERACTIVE)

run-chrome: image-chrome synthetic-network # Run syntheticnet:chrome. Prereq: create-synthetic-network
	$(DOCKER) rm $(CONTAINER_NAME_CHROME) || true
	$(DOCKER) create --privileged \
		--env SYNTHETIC_NETWORK=$(SYNTHETIC_NETWORK) \
		--publish 3000:80 \
		--publish 5901:5901 \
		--name $(CONTAINER_NAME_CHROME) \
	        --tty --interactive \
		$(shell [ -n $(TESTHOST) ] && echo '--add-host="$(TESTHOST)"') \
		syntheticnet:chrome
	$(DOCKER) network connect synthetic-network $(CONTAINER_NAME_CHROME)
	@echo
	@echo "🎛 Synthetic network GUI will listen on http://localhost:3000"
	@echo
	@echo "📺 Point your VNC client at localhost:5901"
	@echo
	$(DOCKER) start --attach --interactive $(CONTAINER_NAME_CHROME)

synthetic-network: # Specify SYNTHETIC_NETWORK (this rule is documentation)
	export SYNTHETIC_NETWORK=$(SYNTHETIC_NETWORK)

create-synthetic-network: synthetic-network # Create Docker network: synthetic-network
	$(DOCKER) network create synthetic-network --subnet=$(SYNTHETIC_NETWORK)

.PHONY: image image-vnc image-chrome minimal rush \
run-interactive run-chrome \
synthetic-network create-synthetic-network
