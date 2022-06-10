#include <cpr/cpr.h>
#include <spdlog/spdlog.h>
//#include "Simple-Web-Server/server_http.hpp"
#include <iostream>
#include <string>
#include <chrono>
#include <vector>
#include <future>
#include <unordered_map>
#include <nlohmann/json.hpp>
using json = nlohmann::json;

std::string SYNCING_JSON = "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"eth_syncing\",\"params\":null}";

struct request_result
{
    int status;
    std::string body;
    cpr::Header headers;
};

class NodeInstance;

struct check_alive_result
{
    NodeInstance *node;
    int status;
    long long resp_time;
};

class NodeInstance
{
public:
    cpr::Url url;
    std::string url_string;
    int status; // 0 = offline, 1 = online, 2 = online but not synced

    NodeInstance(std::string url)
    {
        this->url = cpr::Url{url};
        this->url_string = url;
        this->status = 0;
    }

    void set_online()
    {
        if (this->status == 1)
        {
            return;
        }
        this->status = 1;
        spdlog::info("Node {} is online", this->url_string);
    }

    void set_offline()
    {
        if (this->status == 0)
        {
            return;
        }
        this->status = 0;
        spdlog::info("Node {} is offline", this->url_string);
    }

    check_alive_result check_alive()
    {
        auto start = std::chrono::high_resolution_clock::now();
        auto response = this->do_request(SYNCING_JSON, cpr::Header{{"Content-Type", "application/json"}});
        auto end = std::chrono::high_resolution_clock::now();
        auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();

        json jsondata = json::parse(response.body);
        if (jsondata["result"] == false)
        {
            this->set_online();
            return check_alive_result{this, 1, elapsed};
        }
        else
        {
            this->set_offline();
            return check_alive_result{this, 2, elapsed};
        }
    }

    request_result do_request(std::string &data, cpr::Header &headers)
    {
        std::string response;
        int status;
        try
        {
            auto r = cpr::Post(
                this->url,
                cpr::Body{data},
                headers);
            response = r.text;
            status = r.status_code;
        }
        catch (const cpr::Error &e)
        {
            this->set_offline();
            std::cerr << "Error: " << e.message << std::endl;
        }
        return request_result{status, response, headers};
    }
};

class NodeRouter
{
public:
    std::vector<NodeInstance> nodes;
    std::vector<NodeInstance> alive;
    std::vector<NodeInstance> dead;
    std::vector<NodeInstance> alive_but_syncing;
    int fcU_invalid_threshold;
    int index;

    NodeRouter(std::vector<std::string> urls, int fcU_invalid_threshold)
    {
        this->fcU_invalid_threshold = fcU_invalid_threshold;
        this->index = 0;
        for (auto url : urls)
        {
            this->nodes.push_back(NodeInstance(url));
        }
        spdlog::info("Initialized with {} nodes", this->nodes.size());
    }

    void recheck()
    {
        std::vector<std::future<check_alive_result>> results;
        for (auto &node : this->nodes)
        {
            results.push_back(std::async(&NodeInstance::check_alive, &node));
        }

        // set the alive nodes to the ones that are online, and sort them by their response time
        this->alive.clear();
        this->dead.clear();
        this->alive_but_syncing.clear();
        std::vector<check_alive_result> alive_node_results;

        for (auto &fut : results)
        {
            auto result = fut.get();
            spdlog::info("node {} with response time {}", result.node->url_string, result.resp_time);

            if (result.status == 1)
            {
                alive_node_results.push_back(result);
            }
            else if (result.status == 2)
            {
                this->dead.push_back(*result.node);
            }
            else
            {
                this->alive_but_syncing.push_back(*result.node);
            }
        }

        // sort the alive nodes by their response time. lower index = faster
        std::sort(alive_node_results.begin(), alive_node_results.end(), [](check_alive_result &a, check_alive_result &b)
                  { return a.resp_time < b.resp_time; });

        for (auto &result : alive_node_results)
        {
            this->alive.push_back(*result.node);
        }
    }

    // get the same node on each call, if node ofline, add 1 to the index and try again
    NodeInstance get_execution_node()
    {
        if (this->alive.size() == 0)
        {
            this->recheck();
            return this->get_execution_node();
        }
        else
        {
            auto node = this->alive[this->index];
            this->index = (this->index + 1) % this->alive.size();
            return node;
        }
    }

    // send to all nodes that are alive and syncing, except for the "primary" node
    // also do it async
    void send_to_alive_and_syncing(std::string &data, cpr::Header &headers, NodeInstance &except_node)
    {
        for (auto &node : this->alive)
        {
            if (node.url_string != except_node.url_string)
            {
                std::async(&NodeInstance::do_request, &node, data, headers);
            }
        }
        for (auto &node : this->alive_but_syncing)
        {
            if (node.url_string != except_node.url_string)
            {
                std::async(&NodeInstance::do_request, &node, data, headers);
            }
        }
    }

    // find what the response has is the most common
    // majority must be at least fcU_invalid_threshold
    // if not, return empty string
    std::string fcU_majority(std::vector<request_result> &results)
    {
        std::map<std::string, int> counts;
        for (auto &resp : results)
        {
            // check if the response is already in the map
            if (counts.find(resp.body) == counts.end())
            {
                counts[resp.body] = 1;
            }
            else
            {
                counts[resp.body]++;
            }
        }

        // find the most common response and check if it is at least fcU_invalid_threshold
        int max_count = 0;
        std::string max_resp;
        for (auto &resp : counts)
        {
            if (resp.second > max_count)
            {
                max_count = resp.second;
                max_resp = resp.first;
            }
        }
        if (max_count / results.size() >= this->fcU_invalid_threshold)
        {
            return max_resp;
        }
        else
        {
            return "";
        }
    }

    void send_to_alive_but_syncing(std::string data, cpr::Header headers)
    {
        for (auto &node : this->alive_but_syncing)
        {
            std::async(&NodeInstance::do_request, &node, data, headers);
        }
    }

    /*
    logic for forkchoiceUpdated
    there are three possible statuses: VALID, INVALID, SYNCING (SYNCING is safe and will stall the CL)
    ONLY return VALID when all nodes return VALID
    if any node returns INVALID, return SYNCING

    to return SYNCING, the body of the response should be: '{"jsonrpc":"2.0","id":1,"result":{"payloadStatus":{"status":"SYNCING","latestValidHash":null,"validationError":null},"payloadId":null}}'

    BUT if fcU_majority returns INVALID, return it
    if there is a resonse of INVALID and no majority, return SYNCING
    */

    request_result fcU_logic(std::vector<request_result> &resps)
    {
        auto maj = this->fcU_majority(resps);
        if (json::parse(maj)["result"]["payloadStatus"]["status"] == "INVALID") // here we see if most responses are INVALID, if so, return INVALID
        {
            return request_result{200, maj, cpr::Header{{"Content-Type", "application/json"}, {"Content-Length", std::to_string(maj.size())}, {"Content-Type", "application/json"}}};
        }

        for (auto &resp : resps) // and here we see if just one response is INVALID, if so, return SYNCING (since it isn't a majority, we can't be sure to reject the block by returning INVALID)
        {
            json j = json::parse(resp.body);
            if (j["result"]["payloadStatus"]["status"] == "INVALID" || j["result"]["payloadStatus"]["status"] == "SYNCING")
            {
                return request_result{resp.status, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"SYNCING\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", resp.headers};
            }
        }

        // if we get here, all responses are VALID
        // send to syncing nodes using the first response
        std::async(&NodeRouter::send_to_alive_but_syncing, this, resps[0].body, resps[0].headers);
        return resps[0];
    }
};

int main()
{
    spdlog::set_pattern("[%Y-%m-%d %H:%M:%S] [%^%l%$] - %v");
    // make a vector of urls
    std::vector<std::string> urls;
    urls.push_back("http://192.168.86.83:8545");
    urls.push_back("http://192.168.86.36:8545");

    // create a node router
    NodeRouter router(urls, 3);
    router.recheck();
}