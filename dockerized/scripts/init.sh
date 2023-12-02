#!/bin/bash

START_PARAMS=""

if [ "$SHOW_NODE_TIMINGS" = "true" ]; then
  START_PARAMS="$START_PARAMS --node-timings "
fi

echo $JWT_SECRET > /scripts/.jwt
executionbackup \
$START_PARAMS \
-p $PORT \
-n $NODES \
--listen-addr 0.0.0.0 \
--jwt-secret /scripts/.jwt \
--log-level $LOG_LEVEL \
--fcu-majority $FCU_MAJORITY
