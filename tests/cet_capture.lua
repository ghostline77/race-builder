-- Run from the repository root with: lua tests/cet_capture.lua
-- Exercise capture events without attaching to a running game.
local events, hotkeys, files, captured = {}, {}, {}, nil
json={encode=function(value) captured=value;return '{}' end}
local originalOpen, originalRemove, originalRename=io.open,os.remove,os.rename
io.open=function(path,mode)
    if mode=='r' then return nil end
    return {write=function(_,text)files[path]=text end,close=function()end}
end
os.remove=function(path)files[path]=nil;return true end
os.rename=function(a,b)if not files[a]then return nil,'missing file'end;files[b]=files[a];files[a]=nil;return true end
registerForEvent=function(name,handler)events[name]=handler end
registerHotkey=function(name,_,handler)hotkeys[name]=handler end
local position={x=100,y=200,z=12}
local vehicle={GetWorldPosition=function()return position end,GetWorldForward=function()return{x=0,y=1,z=0}end}
local mounted=true
local player={GetMountedVehicle=function()return mounted and vehicle or nil end,GetWorldPosition=function()return position end,GetWorldForward=function()return{x=-1,y=0,z=0}end}
Game={GetPlayer=function()return player end}
dofile('cet/init.lua')
events.onInit();hotkeys.race_record();events.onUpdate()
assert(captured.format=='ghostline-race-route' and captured.schema_version==1)
assert(#captured.marks==1 and captured.marks[1].evidence.source=='cet-vehicle')
assert(captured.marks[1].yaw==0 and captured.marks[1].position.z==12)
position={x=100,y=205,z=12};events.onUpdate();assert(#captured.marks==1)
position={x=100,y=220,z=13};events.onUpdate();assert(#captured.marks==2)
mounted=false;hotkeys.race_finish()
assert(#captured.marks==3 and captured.marks[3].role=='finish' and captured.marks[3].yaw==90)
assert(captured.marks[3].evidence.source=='cet-player')
hotkeys.race_finish();assert(#captured.marks==3)
events.onUpdate();assert(#captured.marks==3)
io.open,os.remove,os.rename=originalOpen,originalRemove,originalRename
print('CET vehicle/on-foot capture, spacing, finish and duplicate protection passed')
