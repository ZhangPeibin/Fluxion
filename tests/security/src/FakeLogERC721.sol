// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract FakeLogERC721 {
    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);

    function ownerOf(uint256) external pure returns (address) {
        return address(0);
    }

    function safeTransferFrom(address from, address to, uint256 tokenId) external {
        emit Transfer(from, to, tokenId);
    }
}
