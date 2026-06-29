#!/bin/bash


docker rm -vf $(docker ps -aq)
docker rmi -f $(docker images -aq)

cd ~/
sudo rm -rf ./eth-dev

cp /media/sf_vm_shared/exp ~/eth-dev -r