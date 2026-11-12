// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract SwapRouterStub {
    function swapExactTokensForTokens(
        uint256 amountIn,
        uint256 amountOutMin,
        address[] calldata path,
        address to,
        uint256 deadline
    ) external returns (uint256[] memory amounts) {
        require(deadline >= block.timestamp, "expired");
        require(path.length >= 2, "path");
        amounts = new uint256[](path.length);
        amounts[0] = amountIn;
        amounts[path.length - 1] = amountOutMin;
        to;
    }
}
