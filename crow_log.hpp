#include <crow.h>
#include <spdlog/spdlog.h>

// reroute the crow logger to the spdlog logger
class SpdLogAdapter : public crow::ILogHandler
{
public:
    SpdLogAdapter()
    {
    }

    void log(std::string message, crow::LogLevel level) override
    {
        switch (level)
        {
        case crow::LogLevel::Debug:
            spdlog::debug(message);
            break;
        case crow::LogLevel::Info:
            spdlog::info(message);
            break;
        case crow::LogLevel::Warning:
            spdlog::warn(message);
            break;
        case crow::LogLevel::Error:
            spdlog::error(message);
            break;
        case crow::LogLevel::Critical:
            spdlog::critical(message);
            break;
        }
    }
};