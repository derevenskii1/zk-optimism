// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import { Script } from "forge-std/Script.sol";
import { ZKL2OutputOracle } from "src/ZKL2OutputOracle.sol";
import { Utils } from "test/helpers/Utils.sol";

contract ZKUpgrader is Script, Utils {
    function run() public {
        // Read configuration from JSON file
        Config memory config = readJson("zkconfig.json");

        // Start broadcasting with the admin private key from environment variables
        vm.startBroadcast(vm.envUint("ADMIN_PK"));

        // Deploy a new instance of ZKL2OutputOracle
        address zkL2OutputOracleImpl = address(new ZKL2OutputOracle());

        // Upgrade and initialize the contract with the new implementation
        upgradeAndInitialize(
            zkL2OutputOracleImpl,
            config,
            address(0), // Placeholder address, adjust as necessary
            bytes32(0), // Placeholder value, adjust as necessary
            0           // Placeholder value, adjust as necessary
        );

        // Stop broadcasting after deployment
        vm.stopBroadcast();
    }
}
