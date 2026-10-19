# Security Policy

ExecutionBackup is an Ethereum execution-layer multiplexer used by node and staking operators. A flaw in it can affect validator liveness and the view of the chain a consensus client sees, so security reports are taken seriously.

## Maintainer and security contact

- **Maintainer and security contact:** [@TennisBowling](https://github.com/TennisBowling)
- **Email:** [tennisbowling@tennisbowling.com](mailto:tennisbowling@tennisbowling.com)

## Reporting a vulnerability

Please **do not** open a public issue, pull request or discussion for a suspected vulnerability.

Report it privately through GitHub (preferred):

1. Go to the [Security tab](https://github.com/TennisBowling/executionbackup/security) of this repository.
2. Click **Report a vulnerability**.
   (Direct link: <https://github.com/TennisBowling/executionbackup/security/advisories/new>)

If you can't use GitHub, email tennisbowling@tennisbowling.com with the subject line "executionbackup security". Don't include full exploit details in a first message if you'd rather agree on a secure channel first.

Please include, where you can:

- The affected version or commit, and how you are running ExecutionBackup (flags, number of upstream nodes, consensus and execution clients in use)
- A description of the issue and its impact (for example: missed attestations, wrong node selected, loss of failover, JWT or authentication bypass, denial of service)
- Steps or a proof of concept to reproduce it

## What to expect

- Acknowledgement of your report, normally within a few days.
- An assessment of the issue, and updates while a fix is prepared.
- A fix released and a GitHub Security Advisory published (with a CVE where appropriate), with credit to the reporter unless you prefer to stay anonymous.
- Coordinated disclosure: please allow a reasonable window to ship a fix before publishing details. If the issue is in an upstream project (an execution client, consensus client or a library), it will be reported to that project's maintainers as part of the process.

## Supported versions

Security fixes are made against the latest release and the `master` branch. Please reproduce against the [latest release](https://github.com/TennisBowling/executionbackup/releases/latest) before reporting.

## Scope

In scope: the ExecutionBackup code in this repository, including its request routing, failover and node-selection logic, JWT handling, and the Docker files under `dockerized/`.

Out of scope: vulnerabilities in the execution or consensus clients themselves, and misconfiguration of your own nodes, unless ExecutionBackup makes the problem worse. Please report those to the relevant project.
