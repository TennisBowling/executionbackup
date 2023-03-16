FROM --platform=linux/amd64 ubuntu:22.04 as failover

WORKDIR /tmp

RUN apt update && apt install unzip wget -y
RUN wget https://github.com/TennisBowling/executionbackup/releases/latest/download/Linux.zip
RUN unzip Linux.zip
RUN chmod +x executionbackup-0.1.1-Linux.sh
RUN ./executionbackup-0.1.1-Linux.sh --skip-license
RUN mkdir /proxy
RUN mv bin/executionbackup /proxy

WORKDIR /proxy

RUN chmod +x executionbackup
COPY ./failover.sh ./
RUN chmod +x failover.sh

CMD ./failover.sh