// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract ImplementationV1 {
    function version() external pure returns (string memory) {
        return "v1";
    }
}

contract EIP1967Proxy {
    bytes32 internal constant IMPLEMENTATION_SLOT = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    constructor(address implementation) {
        assembly {
            sstore(IMPLEMENTATION_SLOT, implementation)
        }
    }

    function upgradeTo(address implementation) external {
        assembly {
            sstore(IMPLEMENTATION_SLOT, implementation)
        }
    }

    fallback() external payable {
        assembly {
            let implementation := sload(IMPLEMENTATION_SLOT)
            calldatacopy(0, 0, calldatasize())
            let ok := delegatecall(gas(), implementation, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch ok
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }

    receive() external payable {}
}
