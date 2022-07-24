//
// Copyright (c) 2017 Christopher M. Kohlhoff (chris at kohlhoff dot com)
//
// Distributed under the Boost Software License, Version 1.0. (See accompanying
// file LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)
//
// Official repository: https://github.com/boostorg/beast
//

// modified a little by me for my use case

#include <boost/beast/core.hpp>
#include <boost/beast/http.hpp>
#include <boost/beast/version.hpp>
#include <boost/asio.hpp>
#include <boost/asio/thread_pool.hpp>
#include <boost/asio/post.hpp>
#include "fields_alloc.hpp"
#include <cpr/cpr.h>
#include <chrono>
#include <unordered_map>
#include <cstdlib>
#include <ctime>
#include <iostream>
#include <memory>
#include <string>

namespace beast = boost::beast;   // from <boost/beast.hpp>
namespace http = beast::http;     // from <boost/beast/http.hpp>
namespace net = boost::asio;      // from <boost/asio.hpp>
using tcp = boost::asio::ip::tcp; // from <boost/asio/ip/tcp.hpp>

// this struct will be passed to the handler
struct HttpRequest
{
    std::string method;
    std::string uri;
    std::string body;
    cpr::Header headers;
};

// this struct will be passed to the handler through a shared_ptr, handler will modify it, then it will be passed back to the server
struct HttpResponse
{
    std::string body;
    int status_code;
    cpr::Header headers;
};

// every request will be posted to the thread pool, and the handler will be called in the thread pool thread.
// it should handle routes with a unordered_map of string(route) and function(handler)
// it should pass HttpRequest and shared_ptr<HttpResponse> to the handler
// then read the HttpResponse and send it to the client

class Http_Worker
{
public:
    Http_Worker(Http_Worker const &) = delete;
    Http_Worker &operator=(Http_Worker const &) = delete;

    Http_Worker(tcp::acceptor &acceptor, std::unordered_map<std::string, std::function<void(HttpRequest, std::shared_ptr<HttpResponse>)>> *routes_) : acceptor_(acceptor), routes_(routes_) {}

    void start()
    {
        accept();
        check_deadline();
    }

private:
    using alloc_t = fields_alloc<char>;
    // using request_body_t = http::basic_dynamic_body<beast::flat_static_buffer<1024 * 1024>>;
    using request_body_t = http::string_body;

    // The acceptor used to listen for incoming connections.
    tcp::acceptor &acceptor_;

    // The socket for the currently connected client.
    tcp::socket socket_{acceptor_.get_executor()};

    // The buffer for performing reads
    beast::flat_static_buffer<8192> buffer_;

    // The allocator used for the fields in the request and reply.
    alloc_t alloc_{8192};

    // The parser for reading the requests
    boost::optional<http::request_parser<request_body_t, alloc_t>> parser_;

    // The timer putting a time limit on requests.
    net::steady_timer request_deadline_{
        acceptor_.get_executor(), (std::chrono::steady_clock::time_point::max)()};

    // The string-based response message.
    boost::optional<http::response<http::string_body, http::basic_fields<alloc_t>>> string_response_;

    // The string-based response serializer.
    boost::optional<http::response_serializer<http::string_body, http::basic_fields<alloc_t>>> string_serializer_;

    // The route map for handling requests.
    std::unordered_map<std::string, std::function<void(HttpRequest, std::shared_ptr<HttpResponse>)>> *routes_;

    void accept()
    {
        // Clean up any previous connection.
        beast::error_code ec;
        socket_.close(ec);
        buffer_.consume(buffer_.size());

        acceptor_.async_accept(
            socket_,
            [this](beast::error_code ec)
            {
                if (ec)
                {
                    accept();
                }
                else
                {
                    // Request must be fully processed within 60 seconds.
                    request_deadline_.expires_after(
                        std::chrono::seconds(60));

                    read_request();
                }
            });
    }

    void read_request()
    {
        // On each read the parser needs to be destroyed and
        // recreated. We store it in a boost::optional to
        // achieve that.
        //
        // Arguments passed to the parser constructor are
        // forwarded to the message object. A single argument
        // is forwarded to the body constructor.
        //
        // We construct the dynamic body with a 1MB limit
        // to prevent vulnerability to buffer attacks.
        //
        parser_.emplace(
            std::piecewise_construct,
            std::make_tuple(),
            std::make_tuple(alloc_));

        http::async_read(
            socket_,
            buffer_,
            *parser_,
            [this](beast::error_code ec, std::size_t)
            {
                if (ec)
                    accept();
                else
                    process_request(parser_->get());
            });
    }
    void process_request(http::request<request_body_t, http::basic_fields<alloc_t>> const &req)
    {
        switch (req.method())
        {
        case http::verb::post:
            call_handler(req);
            break;

        default:
            // We return responses indicating an error if
            // we do not recognize the request method.
            send_bad_response(
                http::status::bad_request,
                "Invalid request-method '" + std::string(req.method_string()) + "'\r\n");
            break;
        }
    }

    void send_bad_response(
        http::status status,
        std::string const &error)
    {
        string_response_.emplace(
            std::piecewise_construct,
            std::make_tuple(),
            std::make_tuple(alloc_));

        string_response_->result(status);
        string_response_->keep_alive(false);
        string_response_->set(http::field::server, "executionbackup-Beast");
        string_response_->set(http::field::content_type, "text/plain");
        string_response_->body() = error;
        string_response_->prepare_payload();

        string_serializer_.emplace(*string_response_);

        http::async_write(
            socket_,
            *string_serializer_,
            [this](beast::error_code ec, std::size_t)
            {
                socket_.shutdown(tcp::socket::shutdown_send, ec);
                string_serializer_.reset();
                string_response_.reset();
                accept();
            });
    }

    void call_handler(http::request<request_body_t, http::basic_fields<alloc_t>> const &req)
    {
        // get the route from the request
        std::string route = req.target().to_string();
        // get the handler from the route
        auto handler = routes_->find(route);
        // if the handler is found
        if (handler != routes_->end())
        {
            // create the HttpRequest object
            HttpRequest http_request = HttpRequest(req.method_string(), req.target().to_string(), req.body(), basic_fields_to_cpr_header(req.base()));
            std::shared_ptr<HttpResponse> response = std::make_shared<HttpResponse>();
            // call the handler
            handler->second(http_request, response);

            // send the response
            string_response_.emplace(
                std::piecewise_construct,
                std::make_tuple(),
                std::make_tuple(alloc_));

            string_response_->result(response->status_code);
            string_response_->keep_alive(true);
            string_response_->set(http::field::server, "executionbackup-Beast");

            // loop through the headers and set them in the string response
            for (auto &header : response->headers)
            {
                string_response_->set(header.first, header.second);
            }
            // set the body
            string_response_->body() = response->body;
            // prepare the payload
            string_response_->prepare_payload();
            // send the response
            string_serializer_.emplace(*string_response_);
            http::async_write(
                socket_,
                *string_serializer_,
                [this](beast::error_code ec, std::size_t)
                {
                    socket_.shutdown(tcp::socket::shutdown_send, ec);
                    string_serializer_.reset();
                    string_response_.reset();
                    accept();
                });
        }
        else
        {
            // if the handler is not found
            send_bad_response(http::status::not_found, "Not found");
        }
    }

    void check_deadline()
    {
        // The deadline may have moved, so check it has really passed.
        if (request_deadline_.expiry() <= std::chrono::steady_clock::now())
        {
            // Close socket to cancel any outstanding operation.
            beast::error_code ec;
            socket_.close();

            // Sleep indefinitely until we're given a new deadline.
            request_deadline_.expires_at(std::chrono::steady_clock::time_point::max());
        }

        request_deadline_.async_wait([this](beast::error_code)
                                     { check_deadline(); });
    }
};

class HttpServer
{
public:
    net::ip::address address;
    tcp::acceptor acceptor;
    int port;
    int num_workers;
    std::shared_ptr<net::io_context> ioc;
    std::vector<Http_Worker> workers;
    std::unordered_map<std::string, std::function<void(HttpRequest, std::shared_ptr<HttpResponse>)>> routes;

    HttpServer(std::string addressstr, int port, int num_workers)
    {
        this->ioc = std::make_shared<net::io_context>();
        this->address = net::ip::make_address(addressstr);
        this->acceptor = tcp::acceptor(ioc, tcp::endpoint(address, port));
        this->port = port;
        this->num_workers = num_workers;
    }

    void run()
    {
        for (int i = 0; i < num_workers; i++)
        {
            workers.push_back(Http_Worker(this->acceptor, &routes));
            workers.back().start();
        }
        ioc->run();
    }
}