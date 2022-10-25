#/bin/bash

echo $JWT_SECRET > /proxy/.jwt
/proxy/executionbackup -p $PORT -n $NODES --listen-addr 0.0.0.0 --jwt-secret /proxy/.jwt --log-level $LOG_LEVEL