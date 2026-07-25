#include <rapidjson/document.h>
#include <rapidjson/writer.h>
#include <rapidjson/stringbuffer.h>
#include <cstdio>
int main() {
    rapidjson::Document d;
    d.Parse("{\"k\":7}");
    rapidjson::StringBuffer sb;
    rapidjson::Writer<rapidjson::StringBuffer> w(sb);
    d.Accept(w);
    std::printf("rapidjson works: %s\n", sb.GetString());
    return 0;
}
