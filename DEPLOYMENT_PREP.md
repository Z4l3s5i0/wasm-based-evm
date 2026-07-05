# Deployment Preparation

## Overview

This section provides an overview of the deployment preparation process for the application. It covers the necessary steps to ensure a smooth deployment and successful integration of the application into the production environment.

## Building the Image for Deployment

We will build two different images. One for the node itself and one acting as a server for gathering metrics and sending the workloads over contender.

The Steps are going to be the same.
1. Build the vm images
 a. install the software. Mostly the same with the difference in the wasix_eth variants, the metrcis server and contender itself
 b. generate the network configuration. This only applies for the node to generate the genesis and validators. 
2. Install the images
3. Run the applications
 a. run the metrics server first
 b. run the nodes
 c. run contender for spamming the transactions


## TODOS

* [ ] create the node image
* [ ] create the server image
* [ ] test the installation of the images on the vm
* [ ] test the metrics server
* [ ] test running the applications
* [ ] test contender
* [ ] Have a separate HIVE testing environment deployed.
