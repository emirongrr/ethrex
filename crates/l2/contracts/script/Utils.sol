// SPDX-License-Identifier: MIT
pragma solidity 0.8.27;

import {Vm} from "forge-std/Vm.sol";

library Utils {
    // Cheatcodes address, 0x7109709ECfa91a80626fF3989D68f67F5b1DD12D.
    address internal constant VM_ADDRESS = address(uint160(uint256(keccak256("hevm cheat code"))));
    Vm internal constant vm = Vm(VM_ADDRESS);

    // Create2Factory deterministic bytecode.
    // https://github.com/Arachnid/deterministic-deployment-proxy
    bytes internal constant CREATE2_FACTORY_BYTECODE =
        hex"604580600e600039806000f350fe7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe03601600081602082378035828234f58015156039578182fd5b8082525050506014600cf3";

    function deployCreate2Factory() internal returns (address) {
        address child;
        bytes memory bytecode = CREATE2_FACTORY_BYTECODE;
        vm.startBroadcast();
        assembly {
            child := create(0, add(bytecode, 0x20), mload(bytecode))
        }
        vm.stopBroadcast();
        require(child != address(0), "Failed to deploy Create2Factory");
        require(child.code.length > 0, "Failed to deploy Create2Factory");
        return child;
    }

    function deployWithCreate2(bytes memory bytecode, bytes32 salt, address create2Factory) internal returns (address) {
        if (bytecode.length == 0) {
            revert("Bytecode is not set");
        }
        address contractAddress = vm.computeCreate2Address(salt, keccak256(bytecode), create2Factory);
        if (contractAddress.code.length != 0) {
            return contractAddress;
        }

        vm.broadcast();
        (bool success, bytes memory data) = create2Factory.call(abi.encodePacked(salt, bytecode));
        contractAddress = bytesToAddress(data);

        if (!success) {
            revert("Failed to deploy contract via create2: create2Factory call failed");
        }

        if (contractAddress == address(0)) {
            revert("Failed to deploy contract via create2: contract address is zero");
        }

        if (contractAddress.code.length == 0) {
            revert("Failed to deploy contract via create2: contract code is empty");
        }

        return contractAddress;
    }

    function bytesToAddress(bytes memory bys) internal pure returns (address addr) {
        assembly {
            addr := mload(add(bys, 20))
        }
    }
}
