#!/bin/bash

echo $JWT_SECRET > /scripts/.jwt
executionbackup \
-p $PORT \
-n $NODES \
--listen-addr 0.0.0.0 \
--jwt-secret /scripts/.jwt \
--log-level $LOG_LEVEL \
--fcu-majority $FCU_MAJORITY \
--node-timings