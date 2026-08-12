add_executable(test-statemachine
    test_statemachine.cpp
    ../src/screensaverstatemachine.cpp
    ../src/screensaverstatemachine.h
)
target_link_libraries(test-statemachine PRIVATE Qt6::Core Qt6::Test)
add_test(NAME state-machine COMMAND test-statemachine)

add_executable(test-configuration
    test_configuration.cpp
    ../src/configuration.cpp
    ../src/configuration.h
)
target_link_libraries(test-configuration PRIVATE Qt6::Core Qt6::Test KF6::ConfigCore)
add_test(NAME configuration COMMAND test-configuration)

add_executable(test-animationstate
    test_animationstate.cpp
    ../src/animationstate.cpp
    ../src/animationstate.h
)
target_link_libraries(test-animationstate PRIVATE Qt6::Core Qt6::Gui Qt6::Test)
add_test(NAME animation-state COMMAND test-animationstate)
