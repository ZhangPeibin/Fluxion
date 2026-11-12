// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract MulticallHelper {
    function multicall(bytes[] calldata data) external returns (bytes[] memory results) {
        results = new bytes[](data.length);
        for (uint256 i = 0; i < data.length; i++) {
            (bool ok, bytes memory result) = address(this).call(data[i]);
            require(ok, "call failed");
            results[i] = result;
        }
    }

    function approve(address, uint256) external pure returns (bool) {
        return true;
    }

    function transfer(address, uint256) external pure returns (bool) {
        return true;
    }
}
