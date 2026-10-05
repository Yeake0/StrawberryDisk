@echo off
call corepack.cmd pnpm %*
exit /b %ERRORLEVEL%
