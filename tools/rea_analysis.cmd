@echo off
setlocal
set DIR=C:\Program Files\FMOD SoundSystem\FMOD Studio 2.03.13
set OUT=%~dp0..\evidence

echo ### libfsbvorbis64.dll — procedures matching codec keywords
call npx.cmd -y rea-agents@latest search "%DIR%\libfsbvorbis64.dll" --pattern "vorbis|sample|decode|channel|packet|codec|fsb" --kind procedures --mode regex --format json > "%OUT%\rea_procedures.json"
echo exit: %ERRORLEVEL%

echo ### libfsbvorbis64.dll — codec string literals
call npx.cmd -y rea-agents@latest search "%DIR%\libfsbvorbis64.dll" --pattern "vorbis|blocksize|granule|Comment|codec" --kind strings --mode regex --format json > "%OUT%\rea_vorbis_strings.json"
echo exit: %ERRORLEVEL%

echo ### libfsbvorbis64.dll — binary overview
call npx.cmd -y rea-agents@latest analyze "%DIR%\libfsbvorbis64.dll" --format json > "%OUT%\fmod_fsbvorbis_analysis.json"
echo exit: %ERRORLEVEL%

echo DONE — evidence written to %OUT%
endlocal
