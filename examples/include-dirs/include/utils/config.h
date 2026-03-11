#pragma once

#include <string>
#include <map>

namespace utils {

/// Simple key-value configuration holder.
class Config {
public:
    void set(const std::string& key, const std::string& value) {
        data_[key] = value;
    }

    std::string get(const std::string& key, const std::string& default_value = "") const {
        auto it = data_.find(key);
        return it != data_.end() ? it->second : default_value;
    }

    bool has(const std::string& key) const {
        return data_.find(key) != data_.end();
    }

    std::size_t size() const { return data_.size(); }

private:
    std::map<std::string, std::string> data_;
};

} // namespace utils
