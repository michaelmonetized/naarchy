return {
 api=1, id="com.naarchy.launch-brand", name="Naarchy launch identity", version="1.0.0",
 description="A native, editable N made from the island's rounded geometry.",
 actions={{id="mark",name="Create Naarchy mark",category="Icons",run=function(ctx,p)
  local s=math.min(ctx.width,ctx.height)/512
  oma.add_shape{kind="rect",x=36*s,y=36*s,width=440*s,height=440*s,radius=112*s,stroke="#00000000",stroke_width=0,fill="#3035FF",name="Cobalt island"}
  oma.add_shape{kind="rect",x=135*s,y=158*s,width=54*s,height=204*s,radius=27*s,stroke="#00000000",stroke_width=0,fill="#F7F6EE",name="N left stem"}
  oma.add_shape{kind="path",points={{192*s,174*s},{323*s,323*s},{290*s,359*s},{160*s,211*s}},closed=true,stroke="#00000000",stroke_width=0,fill="#F7F6EE",name="N bridge"}
  oma.add_shape{kind="rect",x=291*s,y=158*s,width=54*s,height=204*s,radius=27*s,stroke="#00000000",stroke_width=0,fill="#F7F6EE",name="N right stem"}
  oma.add_shape{kind="ellipse",x=318*s,y=111*s,width=57*s,height=57*s,stroke="#00000000",stroke_width=0,fill="#D5FE6B",name="Ready light"}
  oma.message("Naarchy mark created. Five editable native shapes.")
 end}}
}
