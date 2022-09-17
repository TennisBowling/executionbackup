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
#include "Simple-Web-Server/server_http.hpp"
using HttpServer = SimpleWeb::Server<SimpleWeb::HTTP>;

std::unordered_map<int, SimpleWeb::StatusCode> status_code_to_enum =
    {
        {100, SimpleWeb::StatusCode::information_continue},
        {101, SimpleWeb::StatusCode::information_switching_protocols},
        {102, SimpleWeb::StatusCode::information_processing},
        {200, SimpleWeb::StatusCode::success_ok},
        {201, SimpleWeb::StatusCode::success_created},
        {202, SimpleWeb::StatusCode::success_accepted},
        {203, SimpleWeb::StatusCode::success_non_authoritative_information},
        {204, SimpleWeb::StatusCode::success_no_content},
        {205, SimpleWeb::StatusCode::success_reset_content},
        {206, SimpleWeb::StatusCode::success_partial_content},
        {207, SimpleWeb::StatusCode::success_multi_status},
        {208, SimpleWeb::StatusCode::success_already_reported},
        {226, SimpleWeb::StatusCode::success_im_used},
        {300, SimpleWeb::StatusCode::redirection_multiple_choices},
        {301, SimpleWeb::StatusCode::redirection_moved_permanently},
        {302, SimpleWeb::StatusCode::redirection_found},
        {303, SimpleWeb::StatusCode::redirection_see_other},
        {304, SimpleWeb::StatusCode::redirection_not_modified},
        {305, SimpleWeb::StatusCode::redirection_use_proxy},
        {307, SimpleWeb::StatusCode::redirection_temporary_redirect},
        {308, SimpleWeb::StatusCode::redirection_permanent_redirect},
        {400, SimpleWeb::StatusCode::client_error_bad_request},
        {401, SimpleWeb::StatusCode::client_error_unauthorized},
        {402, SimpleWeb::StatusCode::client_error_payment_required},
        {403, SimpleWeb::StatusCode::client_error_forbidden},
        {404, SimpleWeb::StatusCode::client_error_not_found},
        {405, SimpleWeb::StatusCode::client_error_method_not_allowed},
        {406, SimpleWeb::StatusCode::client_error_not_acceptable},
        {407, SimpleWeb::StatusCode::client_error_proxy_authentication_required},
        {408, SimpleWeb::StatusCode::client_error_request_timeout},
        {409, SimpleWeb::StatusCode::client_error_conflict},
        {410, SimpleWeb::StatusCode::client_error_gone},
        {411, SimpleWeb::StatusCode::client_error_length_required},
        {412, SimpleWeb::StatusCode::client_error_precondition_failed},
        {413, SimpleWeb::StatusCode::client_error_payload_too_large},
        {414, SimpleWeb::StatusCode::client_error_uri_too_long},
        {415, SimpleWeb::StatusCode::client_error_unsupported_media_type},
        {416, SimpleWeb::StatusCode::client_error_range_not_satisfiable},
        {417, SimpleWeb::StatusCode::client_error_expectation_failed},
        {418, SimpleWeb::StatusCode::client_error_im_a_teapot},
        {421, SimpleWeb::StatusCode::client_error_misdirection_required},
        {422, SimpleWeb::StatusCode::client_error_unprocessable_entity},
        {423, SimpleWeb::StatusCode::client_error_locked},
        {424, SimpleWeb::StatusCode::client_error_failed_dependency},
        {426, SimpleWeb::StatusCode::client_error_upgrade_required},
        {428, SimpleWeb::StatusCode::client_error_precondition_required},
        {429, SimpleWeb::StatusCode::client_error_too_many_requests},
        {431, SimpleWeb::StatusCode::client_error_request_header_fields_too_large},
        {451, SimpleWeb::StatusCode::client_error_unavailable_for_legal_reasons},
        {500, SimpleWeb::StatusCode::server_error_internal_server_error},
        {501, SimpleWeb::StatusCode::server_error_not_implemented},
        {502, SimpleWeb::StatusCode::server_error_bad_gateway},
        {503, SimpleWeb::StatusCode::server_error_service_unavailable},
        {504, SimpleWeb::StatusCode::server_error_gateway_timeout},
        {505, SimpleWeb::StatusCode::server_error_http_version_not_supported},
        {506, SimpleWeb::StatusCode::server_error_variant_also_negotiates},
        {507, SimpleWeb::StatusCode::server_error_insufficient_storage},
        {508, SimpleWeb::StatusCode::server_error_loop_detected},
        {510, SimpleWeb::StatusCode::server_error_not_extended},
        {511, SimpleWeb::StatusCode::server_error_network_authentication_required}};

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

        ("help,h", "produce help message")                                                                                                             // help message
        ("version,v", "print version")                                                                                                                 // version message
        ("port,p", boost::program_options::value<int>(), "port to listen on")                                                                          // port to listen on
        ("nodes,n", boost::program_options::value<std::string>(), "comma separated list of nodes")                                                     // comma separated list of nodes
        ("fcu-invalid-threshold,fcu", boost::program_options::value<double>(), "fcU invalid threshold")                                                // fcU invalid threshold
        ("listen-addr,addr", boost::program_options::value<std::string>(), "address to listen on for json-rpc")                                        // listen addr
        ("log-level", boost::program_options::value<std::string>(), "verbosity of the program. Possible values: TRACE DEBUG INFO WARN ERROR CRITICAL") // log level
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
        std::cout << "executionbackup version 1.0.4\n";
        std::cout << "Compiled with " << BOOST_COMPILER << std::endl;
        std::cout << "Made with love by tennis ;) <3" << std::endl;
        exit(0);
    }

    if (vm.count("log-level"))
    {
        std::string log_level = vm["log-level"].as<std::string>();
        if (log_level == "TRACE")
        {
            spdlog::set_level(spdlog::level::trace);
        }
        else if (log_level == "DEBUG")
        {
            spdlog::set_level(spdlog::level::debug);
        }
        else if (log_level == "INFO")
        {
            spdlog::set_level(spdlog::level::info);
        }
        else if (log_level == "WARN")
        {
            spdlog::set_level(spdlog::level::warn);
        }
        else if (log_level == "ERROR")
        {
            spdlog::set_level(spdlog::level::err);
        }
        else if (log_level == "CRITICAL")
        {
            spdlog::set_level(spdlog::level::critical);
        }
        else
        {
            spdlog::error("Invalid log level: {}", log_level);
            exit(1);
        }
    }
    else
    {
        spdlog::set_level(spdlog::level::info);
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
