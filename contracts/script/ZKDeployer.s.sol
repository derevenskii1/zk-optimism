// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import { Script } from "forge-std/Script.sol";
import { ZKL2OutputOracle } from "src/ZKL2OutputOracle.sol";
import { Utils } from "test/helpers/Utils.sol";
import { Proxy } from "@optimism/src/universal/Proxy.sol";

contract ZKDeployer is Script, Utils {
    function run() public {
        // Start broadcasting to deploy contracts
        vm.startBroadcast();

        // Read configuration from JSON file
        Config memory config = readJson("zkconfig.json");

        // Deploy Proxy contract with the sender address as the implementation
        address proxyAddress = address(new Proxy(msg.sender));
        config.l2OutputOracleProxy = proxyAddress;

        // Deploy ZKL2OutputOracle implementation contract
        address zkL2OutputOracleImpl = address(new ZKL2OutputOracle());

        // Call upgrade and initialize function with appropriate parameters
        upgradeAndInitialize(
            zkL2OutputOracleImpl,
            config,
            address(0), // Adjust as needed for the specific use case
            bytes32(0), // Adjust as needed for the specific use case
            0           // Adjust as needed for the specific use case
        );

        // Stop broadcasting
        vm.stopBroadcast();
    }
}
