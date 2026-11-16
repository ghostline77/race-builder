-- Route authoring only: read player/vehicle transforms; never control gameplay.
local route, recording, visible = nil, false, false
local roleIndex, spacing, status = 0, 12.0, 'Ready'
local roles = {'spline_point','checkpoint','start_grid','player_start','staging','activation','finish','opponent_exit','recovery','reward','spectator','hazard','boundary','gps','prop','speed_change','width_change'}
local counter = 0
local atan2=math.atan2 or function(y,x)return math.atan(y,x)end
local function newRoute()
    recording=false
    route={format='ghostline-race-route',schema_version=1,id='route_'..os.time()..'_'..math.random(1000,9999),title='Captured race',description='',prefix='races',looped=false,width_default=5.0,speed_mps=100.0,marks={}}
end
local function save()
    local path='route.json'; local file,reason=io.open(path..'.tmp','w')
    if not file then status='Save failed: '..tostring(reason); return false end
    local raw=json.encode(route):gsub('"marks":%s*{}','"marks":[]')
    file:write(raw); file:close()
    local old=io.open(path,'r')
    if old then
        old:close();os.remove(path..'.bak')
        local moved,reason=os.rename(path,path..'.bak')
        if not moved then status='Save failed: '..tostring(reason);return false end
    end
    local ok,error=os.rename(path..'.tmp',path)
    if not ok and old then os.rename(path..'.bak',path) end
    if ok then os.remove(path..'.bak') end
    status=ok and 'Saved route.json' or ('Save failed: '..tostring(error))
    return ok
end
local function transform()
    local player=Game.GetPlayer()
    if not player then error('Load a game first') end
    local vehicle=player:GetMountedVehicle()
    local object=vehicle or player
    local p=object:GetWorldPosition(); local forward=object:GetWorldForward()
    return {x=p.x,y=p.y,z=p.z},(math.deg(atan2(-forward.x,forward.y))+360)%360,vehicle and 'cet-vehicle' or 'cet-player'
end
local function add(role)
    local ok,error=pcall(function()
        local p,yaw,source=transform(); local order=0
        for _,m in ipairs(route.marks) do
            if m.role==role then
                if role=='finish' or role=='player_start' or role=='staging' or role=='activation' then error('Remove the existing '..role..' before replacing it') end
                order=math.max(order,m.order+1)
            end
        end
        counter=counter+1
        local options={}
        if role=='speed_change' then options.speed_mps=route.speed_mps end
        if role=='width_change' then options.left_m=route.width_default; options.right_m=route.width_default end
        if role=='checkpoint' or role=='staging' or role=='finish' or role=='hazard' or role=='boundary' then options.width_m=10; options.depth_m=2; options.height_m=4 end
        route.marks[#route.marks+1]={id='mark_'..os.time()..'_'..counter,role=role,order=order,name='',notes='',position=p,yaw=yaw,options=options,evidence={source=source}}
        save()
    end)
    if not ok then status=tostring(error) end
end
registerForEvent('onInit',function()
    math.randomseed(os.time()); newRoute()
    local file=io.open('route.json','r')
    if file then
        local raw=file:read('*a'); file:close(); local ok,value=pcall(json.decode,raw)
        if ok and value.format=='ghostline-race-route' and value.schema_version==1 then route=value;counter=#route.marks;status='Loaded route.json' end
    end
end)
registerForEvent('onOverlayOpen',function() visible=true end)
registerForEvent('onOverlayClose',function() visible=false end)
registerForEvent('onUpdate',function()
    if not recording or not route then return end
    local ok,p=pcall(transform); if not ok then return end
    local last=nil
    for _,m in ipairs(route.marks) do if m.role=='spline_point' and (not last or m.order>last.order) then last=m end end
    if not last or math.sqrt((p.x-last.position.x)^2+(p.y-last.position.y)^2+(p.z-last.position.z)^2)>=spacing then add('spline_point') end
end)
registerHotkey('race_mark','Race Builder: add selected mark',function() if route then add(roles[roleIndex+1]) end end)
registerHotkey('race_record','Race Builder: toggle course recording',function() recording=not recording; status=recording and 'Recording course' or 'Recording stopped' end)
registerHotkey('race_finish','Race Builder: mark finish',function() if route then recording=false; add('finish') end end)
registerForEvent('onDraw',function()
    if not visible or not route then return end
    if ImGui.Begin('Race Builder') then
        route.title=ImGui.InputText('Title',route.title,256)
        route.description=ImGui.InputText('Description',route.description,2048)
        route.looped=ImGui.Checkbox('Closed course',route.looped)
        roleIndex=ImGui.Combo('Mark role',roleIndex,roles,#roles)
        spacing=ImGui.SliderFloat('Recording spacing (m)',spacing,2,100,'%.1f')
        if ImGui.Button(recording and 'Stop recording' or 'Record course while driving') then recording=not recording end
        if ImGui.Button('Add mark at player / mounted vehicle') then add(roles[roleIndex+1]) end
        ImGui.Text('Activation is a standing workspot root: capture it on foot on clear ground.')
        if ImGui.Button('Save route.json') then save() end
        ImGui.SameLine()
        if ImGui.Button('Archive current and start new') then
            local file=io.open(route.id..'.route.json','w')
            if file then file:write(json.encode(route):gsub('"marks":%s*{}','"marks":[]'));file:close();newRoute();save() else status='Could not archive the current route' end
        end
        ImGui.Separator(); ImGui.Text(status)
        for i,m in ipairs(route.marks) do
            ImGui.PushID(i)
            ImGui.Text(m.role..' '..(m.order+1)..string.format('  %.1f, %.1f, %.1f',m.position.x,m.position.y,m.position.z))
            ImGui.SameLine()
            if ImGui.SmallButton('Remove') then table.remove(route.marks,i);save();ImGui.PopID();break end
            ImGui.PopID()
        end
    end
    ImGui.End()
end)
