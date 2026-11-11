# inspect.decode - Calldata Decoding

## Goal

Turn calldata into a stable intent: selector, signature, function name and typed arguments.

## Dependencies

`inspect.core`.

## Built-in Selectors

Ship a small, reviewed registry first:

- `transfer(address,uint256)` `0xa9059cbb`
- `approve(address,uint256)` `0x095ea7b3`
- `transferFrom(address,address,uint256)` `0x23b872dd`
- `increaseAllowance(address,uint256)` `0x39509351`
- `decreaseAllowance(address,uint256)` `0xa457c2d7`
- `setApprovalForAll(address,bool)` `0xa22cb465`
- `safeTransferFrom(address,address,uint256)` `0x42842e0e`
- `safeTransferFrom(address,address,uint256,bytes)` `0xb88d4fde` (dynamic bytes supported)
- `safeTransferFrom(address,address,uint256,uint256,bytes)` `0xf242432a` (ERC1155, dynamic bytes supported)
- `multicall(bytes[])` `0xac9650d8` (decoded by the batch spec)
- `deposit()` `0xd0e30db0`
- `withdraw(uint256)` `0x2e1a7d4d`
- `transferOwnership(address)` `0xf2fde38b`
- `upgradeTo(address)` `0x3659cfe6`
- `permit(address,address,uint256,uint256,uint8,bytes32,bytes32)` `0xd505accf`

The registry is append-only. A selector collision must produce `unknown`, never an arbitrary
winner.

## Rules

- `DECODE-001`: unknown selector -> `signature: null`, `name: null`, no args.
- `DECODE-002`: known selector with malformed argument length -> unknown args plus a warning.
- `DECODE-003`: dynamic arguments (bytes/string/arrays) must not be truncated silently.
- `DECODE-004`: selector collision -> unknown and a warning; the built-in registry is test-enforced
  unique.

## Argument Model

```json
{"name": "spender", "type": "address", "value": "0x..."}
```

Values are canonical strings. `uint256` uses decimal. `bytes` uses `0x` hex. Dynamic values may be
summarized when full decoding is not available, but the summary must be explicit.

## Failure and Unknown Behaviour

- No ABI source is required for built-in selectors.
- Dynamic `bytes` is decoded for ERC721/ERC1155 safe transfers. `bytes[]` multicall decoding belongs
  to the batch detector.
- Contract-specific ABI lookup is a later extension; unknown remains valid.
- Proxy target decoding is handled by `inspect.metadata`, not guessed here.

## Tests

- every built-in selector decodes a valid fixture;
- malformed static arguments produce unknown, not panic;
- unknown selector is stable;
- dynamic argument fixture is preserved or explicitly rejected;
- selector collision fixture.

## Acceptance

- known selector output matches schema;
- unknown selector is not assigned a signature;
- no network access in default tests.
