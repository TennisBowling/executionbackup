# consensusbackup

consensusbackup is a high-availability routing consensus node program.  
consensusbackup supports the whole consensus standard API, with any client that supports the standard API can run as a node for consensusbackup.
consensusbackup load-balances the network of nodes, and provides a reliable and high-performance consensus service.

## Installing
Install latest stable:
`python -m pip install consensusbackup`  

Install latest develop:
`python -m pip install git+https://github.com/TennisBowling/consensusbackup.git`  


## Usage
Example running in examplerunner.py  
You'll need to add your nodes to the list in the runner.  
Then run it like `python examplerunner.py`  