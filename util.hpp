#pragma once
#include <vector>
#include <string>
#include <future>
#include <unordered_map>
#include <fstream>
#include <sstream>
#include <iostream>
#include <cpr/cpr.h>
#include <boost/program_options.hpp>
#include <boost/config.hpp>
#include <spdlog/spdlog.h>

//#include "Simple-Web-Server/server_http.hpp"
// using HttpServer = SimpleWeb::Server<SimpleWeb::HTTP>;

template <typename T>
std::vector<T> join_all_async(std::vector<std::future<T>> &futures)
{
    std::vector<T> result;
    for (auto &future : futures)
    {
        result.push_back(future.get());
    }
    return result;
}

// get all header name and pairs and add them to a cstring, separating each pair with a semicolon
const char *cpr_header_to_cstr(cpr::Header &header)
{
    std::stringstream ss;
    for (auto &pair : header)
    {
        ss << pair.first << ": " << pair.second << ";";
    }
    ss << "\r\n";
    return ss.str().c_str();
}

void csv_to_vec(std::string csv, std::vector<std::string> &vec)
{
    std::stringstream ss(csv);
    std::string item;
    while (std::getline(ss, item, ','))
    {
        vec.push_back(item);
    }
}

std::string read_jwt(const std::string &filepath)
{
    std::ifstream filestream(filepath);

    if (filestream.is_open())
    {
        std::string jwt;
        filestream >> jwt;

        if (!jwt.starts_with("0x"))
        {
            spdlog::critical("JWT token is not properly formatted");
        }

        jwt.erase(0, 2); // remove the "0x" prefix
        return jwt;
    }
    else
    {
        spdlog::error("Unable to open file {} for the JWT secret.", filepath);
        exit(1);
    }
}

boost::program_options::variables_map parse_args(int argc, char *argv[])
{
    // we need to accept the following arguments:
    // -p, --port <port>
    // --nodes <nodes> (comma separated list of nodes)
    // --fcu-invalid-threshold <threshold>
    // -h, --help
    // -v, --version

    boost::program_options::options_description desc("Allowed options");
    desc.add_options()

        ("help,h", "produce help message")                                                                      // help message
        ("version,v", "print version")                                                                          // version message
        ("port,p", boost::program_options::value<int>(), "port to listen on")                                   // port to listen on
        ("nodes,n", boost::program_options::value<std::string>(), "comma separated list of nodes")              // comma separated list of nodes
        ("fcu-invalid-threshold,fcu", boost::program_options::value<double>(), "fcU invalid threshold")         // fcU invalid threshold
        ("listen-addr,addr", boost::program_options::value<std::string>(), "address to listen on for json-rpc") // listen addr
        ("jwt-secret,jwt", boost::program_options::value<std::string>(), "The file path for the jwt secret file");

    boost::program_options::variables_map vm;
    boost::program_options::store(boost::program_options::parse_command_line(argc, argv, desc), vm);
    boost::program_options::notify(vm);

    if (vm.count("help"))
    {
        std::cout << desc << std::endl;
        exit(0);
    }

    if (vm.count("version"))
    {
        std::cout << "executionbackup version 1.0.2\n";
        std::cout << "Compiled with " << BOOST_COMPILER << std::endl;
        std::cout << "Made with love by tennis ;) <3" << std::endl;
        exit(0);
    }

    if (vm.count("nodes") == 0)
    {
        spdlog::critical("No nodes specified, exiting.");
        exit(1);
    }

    if (vm.count("jwt-secret") == 0)
    {
        spdlog::critical("No jwt secret specified, exiting.");
        exit(1);
    }

    if (vm.count("port") == 0)
    {
        spdlog::warn("No port specified, using default port 8000.");
    }

    return vm;
}
