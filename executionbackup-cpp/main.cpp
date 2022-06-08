#include <cpr/cpr.h>
#include <spdlog/spdlog.h>
#include <iostream>
#include <string>
#include <chrono>
#include <vector>
#include <future>
#include <unordered_map>
#include <nlohmann/json.hpp>
using json = nlohmann::json;

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
        auto response = this->do_request("{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"eth_syncing\",\"params\":null}", cpr::Header{{"Content-Type", "application/json"}});
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

    request_result do_request(std::string data, cpr::Header headers)
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
            std::cout << "bruh" << std::endl;
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
    void send_to_alive_and_syncing(std::string data, cpr::Header headers, NodeInstance except_node)
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