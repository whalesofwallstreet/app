# Whales of Wallstreet

The Whales of Wallstreet (WOW) ecosystem is a multi-product financial routing and simulation platform.

It consists of:

- Marketing Site (this docs site)
- Web App
- Native App
- Wow Engine (core backend)

---

## Architecture

The system is built around a shared backend called Wow Engine.

It provides:

- Cross-chain routing
- Bridge aggregation
- Stellar anchor onboarding
- Fiat on/off ramp infrastructure

---

## System Flow

Web App and Native App both communicate with Wow Engine which executes routing across:

- Ethereum
- Solana
- Arbitrum
- Stellar Anchors