#!/bin/bash
cd /path_to/job
./job 

## chmod +x task.sh
## crontab -e:  0 9 * * * /path_to/task.sh >> /path_to/srv.log 2>&1
## nohup ./web &
## nohup ./worker &
