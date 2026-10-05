import {readFileSync,writeFileSync} from 'node:fs';
const logo=JSON.parse(readFileSync('launch/brand/naarchy-logo.oma','utf8'));
logo.doc.name='Naarchy — The little island';
logo.doc.workspace='motion';
const shapes=logo.doc.layers.flatMap((l:any)=>l.kind.Vector?.shapes??[]);
const tracks:any[]=[];
for(const [i,s] of shapes.entries()){
 const t=i*.12;
 tracks.push({shape:s.id,prop:'Opacity',keys:[{t:0,value:0,ease:'Linear'},{t:t+.05,value:0,ease:'Linear'},{t:t+.55,value:1,ease:'EaseOut'},{t:3.6,value:1,ease:'Linear'},{t:4,value:0,ease:'EaseIn'}]});
 tracks.push({shape:s.id,prop:'Y',keys:[{t:0,value:65,ease:'Linear'},{t:t+.05,value:65,ease:'Linear'},{t:t+.55,value:0,ease:'EaseOut'},{t:3.6,value:0,ease:'Linear'},{t:4,value:-35,ease:'EaseIn'}]});
}
logo.doc.motion={duration:4,fps:30,looped:true,tracks};
writeFileSync('launch/motion/naarchy-reveal.oma',JSON.stringify(logo));
