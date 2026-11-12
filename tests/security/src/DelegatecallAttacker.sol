// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract DelegatecallAttacker {
    function claim(address target, bytes calldata data) external returns (bytes memory) {
        (bool ok, bytes memory result) = target.delegatecall(data);
        require(ok, "delegatecall failed");
        return result;
    }
}

contract BenignTarget {
    event Ping(address indexed sender);

    function ping() external {
        emit Ping(msg.sender);
    }
}
