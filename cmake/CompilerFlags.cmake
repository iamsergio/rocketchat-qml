# SPDX-FileCopyrightText: 2026 Sergio Martins
#
# SPDX-License-Identifier: MIT

function(rocketchatqml_set_compiler_flags target developer_mode)
  if(developer_mode)
    if(MSVC)
      target_compile_options(${target} PRIVATE /W4)
    elseif(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
      target_compile_options(${target} PRIVATE -Wall -Wextra -Werror)
    endif()
  endif()
endfunction()
