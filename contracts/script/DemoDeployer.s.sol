// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import { Script } from "forge-std/Script.sol";
import { DemoZKL2OutputOracle } from "src/DemoZKL2OutputOracle.sol";
import { Utils } from "test/helpers/Utils.sol";
import { Proxy }  from "@optimism/src/universal/Proxy.sol";

contract DemoDeployer is Script, Utils {
    function run() public {
        vm.startBroadcast();

        DemoZKL2OutputOracle zkl2oo = new DemoZKL2OutputOracle();

        Config memory config = readJson("zkconfig.json");
        if (config.chainId != 0) {
            (bytes32 startingOutputRoot, uint startingTimestamp) = fetchOutputRoot(config);

            DemoZKL2OutputOracle.ZKInitParams memory zkInitParams = DemoZKL2OutputOracle.ZKInitParams({
                chainId: config.chainId,
                vkey: config.vkey,
                verifierGateway: config.verifierGateway,
                startingOutputRoot: startingOutputRoot,
                owner: config.owner
            });

            zkl2oo.initialize({
                _submissionInterval: config.submissionInterval,
                _l2BlockTime: config.l2BlockTime,
                _startingBlockNumber: config.startingBlockNumber,
                _startingTimestamp: startingTimestamp,
                _proposer: config.proposer,
                _challenger: config.challenger,
                _finalizationPeriodSeconds: config.finalizationPeriod,
                _zkInitParams: zkInitParams
            });
        }

        vm.stopBroadcast();
    }
}
