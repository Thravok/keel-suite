cmake_minimum_required(VERSION 3.21)
project(keel-light-ui LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

find_package(Qt6 REQUIRED COMPONENTS Widgets WebSockets Network)

set(KEEL_LIGHT_UI_SOURCES
  main.cpp
  MainWindow.cpp
  MainWindow.h
  RpcClient.cpp
  RpcClient.h
)

if(APPLE)
  enable_language(OBJC)
  list(APPEND KEEL_LIGHT_UI_SOURCES
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../packaging/macos/sparkle_bridge.m"
  )
endif()

add_executable(keel-light-ui ${KEEL_LIGHT_UI_SOURCES})

target_include_directories(keel-light-ui PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}/../../../packaging/macos"
)

target_link_libraries(keel-light-ui PRIVATE Qt6::Widgets Qt6::WebSockets Qt6::Network)

if(APPLE)
  target_link_libraries(keel-light-ui PRIVATE "-framework Foundation")
  target_compile_options(keel-light-ui PRIVATE $<$<COMPILE_LANGUAGE:OBJC>:-fobjc-arc>)
endif()

install(TARGETS keel-light-ui RUNTIME DESTINATION bin)
