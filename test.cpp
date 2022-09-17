#include <map>
#include <string>
#include <algorithm>
#include <iostream>
#include <map>
#include <vector>

struct request_result
{
    int status;
    std::string body;
    std::map<std::string, std::string> headers;
};

std::string fcU_majority(std::vector<request_result> &results)
{
    std::map<std::string, int> responses;
    for (auto &result : results)
    {
        responses[result.body]++;
    }
    auto max_response = std::max_element(responses.begin(), responses.end(), [](const std::pair<std::string, int> &a, const std::pair<std::string, int> &b)
                                         { return a.second < b.second; });
    if (max_response->second > results.size() * 1.0)
    {
        return max_response->first;
    }
    else
    {
        return "";
    }
}

request_result fcU_logic(std::vector<request_result> &resps)
{
    auto maj = fcU_majority(resps);

    if (maj.empty()) // if there is no majority, return SYNCING
    {
        return request_result{200, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"SYNCING\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", std::map{{"Content-Type", "application/json"}, {"Content-Length", "135"}, {"Content-Type", "application/json"}}};
    }

    if (json::parse(maj)["result"]["payloadStatus"]["status"] == "INVALID") // here we see if most responses are INVALID, if so, return INVALID
    {
        return request_result{200, maj, std::map{{"Content-Type", "application/json"}, {"Content-Length", std::to_string(maj.size())}, {"Content-Type", "application/json"}}};
    }

    for (auto &resp : resps) // and here we see if just one response is INVALID, if so, return SYNCING (since it isn't a majority, we can't be sure to reject the block by returning INVALID)
    {
        json j = json::parse(resp.body);
        if (j["result"]["payloadStatus"]["status"] == "INVALID" || j["result"]["payloadStatus"]["status"] == "SYNCING")
        {
            return request_result{200, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"SYNCING\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", std::map{{"Content-Type", "application/json"}, {"Content-Length", "135"}, {"Content-Type", "application/json"}}}; // we cannot use the response's headers since we are replacing the body with our own (invalidating the content type)
        }
    }

    return resps[0];
}

int main()
{
    std::vector<request_result> resps;
    resps.push_back(request_result{200, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"VALID\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", std::map{{"Content-Type", "application/json"}, {"Content-Length", "135"}, {"Content-Type", "application/json"}}});
    resps.push_back(request_result{200, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"VALID\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", std::map{{"Content-Type", "application/json"}, {"Content-Length", "135"}, {"Content-Type", "application/json"}}});
    resps.push_back(request_result{200, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"VALID\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", std::map{{"Content-Type", "application/json"}, {"Content-Length", "135"}, {"Content-Type", "application/json"}}});
    resps.push_back(request_result{200, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"VALID\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", std::map{{"Content-Type", "application/json"}, {"Content-Length", "135"}, {"Content-Type", "application/json"}}});
    resps.push_back(request_result{200, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"payloadStatus\":{\"status\":\"INVALID\",\"latestValidHash\":null,\"validationError\":null},\"payloadId\":null}}", std::map{{"Content-Type", "application/json"}, {"Content-Length", "135"}, {"Content-Type", "application/json"}}});

    auto result = fcU_logic(resps);
    std::cout << result.body << std::endl;
}