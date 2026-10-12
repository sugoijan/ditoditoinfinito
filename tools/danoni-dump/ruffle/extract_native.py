"""Export hash-bound saved native captures through verified engine-family contracts."""
import argparse
import json
from pathlib import Path
import shutil

from export_io import digest,write_json
from paths import external_directory
from rebuild_corpus import checked,copy_tree
from native_families import dac_five_projection,constant_projection,review_optional_colors,extract_movie_arrays,scheduled_projection,movie_observation,movie_audio_origin,relative_chart,audit_ok
from native_audio import canonical_audio
from swf_audio import demux,read_movie,iter_tags


def capture(root,binding,swf_hash,inputs):
    path=checked(root,binding['path'],binding['sha256']);directory=path.parent
    parent_path=checked(root,str((directory/'run.json').relative_to(root)),binding['runSha256'])
    parent=json.loads(parent_path.read_text());data=json.loads(path.read_text())
    if 'pcmSha256' in binding: checked(root,str((directory/'audio.pcm').relative_to(root)),binding['pcmSha256'])
    if parent['exitCode'] or parent['termination'] or not parent['inputsUnchanged']:raise ValueError('Failed restricted native run')
    if data['source']['movieSha256']!=swf_hash:raise ValueError('Capture belongs to another source movie')
    configs=[p for p in inputs.glob('*.json') if digest(p.read_bytes())==parent['configSha256']]
    if not configs or any(json.loads(p.read_text())!=data['source']['config'] for p in configs):raise ValueError('Capture config is not bound to preserved inputs')
    if data['ruffleRevision']!='89a7049965f3090e706265670df309a5128233ac':raise ValueError('Unreviewed native runtime')
    return path,data,parent


def stream_path(movie,start):
    _,data,header=read_movie(movie)
    # The observed slice end identifies its root/sprite timeline; a containing
    # MP3 head must match the captured decoder's format and precede the slice.
    # Ruffle SwfSlice offsets index decompressed tag data, excluding the SWF
    # header/RECT/FPS fields. The bounded static parser reports full-file offsets.
    first=start['sliceStart']+header['tagStart'];last=start['sliceEnd']+header['tagStart']
    candidates=set();ends={}
    for tag in iter_tags(data,header['tagStart'],len(data)):
        if tag['code']==0:ends[tag['path']]=tag['offset']+2
        if tag['code']==19 and first<=tag['offset']<last:candidates.add(tag['path'])
    matches=[]
    for path in sorted(candidates):
        if ends.get(path)!=last:continue
        try:_,encoded,report=demux(movie,path,first,last)
        except ValueError:continue
        if report['sampleRate']==start['sampleRate'] and (report['channels']==2)==start['stereo'] and report['latencySeek']==start['latencySeek'] and report['ruffleContinuousPayloadEligible']:
            report['nativeSlice']={'tagDataOffsetBase':header['tagStart'],'sliceStart':start['sliceStart'],'sliceEnd':start['sliceEnd']}
            matches.append((path,encoded,report))
    if len(matches)!=1:raise ValueError('Observed audio slice does not identify one continuous song stream')
    return matches[0]


def extract(recipe_path, output, ffmpeg):
    raw_recipe=recipe_path.read_bytes();recipe=json.loads(raw_recipe)
    if recipe.get('schema')!='ddi-native-family-recipe/experimental-1':raise ValueError('Unknown native recipe')
    root=external_directory(recipe['root']).resolve(strict=True)
    source_path=checked(root,recipe['source']['path'],recipe['source']['sha256']);source=json.loads(source_path.read_text())
    inputs=(root/recipe['inputs']).resolve(strict=True)
    if Path(recipe['inputs']).is_absolute() or not inputs.is_relative_to(root):raise ValueError('Input directory must remain inside corpus')
    swf_hash=source.get('sourceSwfSha256',source.get('swf',{}).get('sha256'))
    if not swf_hash:raise ValueError('Need source movie hash and provenance')
    output=external_directory(output);output.mkdir(parents=True,exist_ok=False)
    (output/'media').mkdir();(output/'references').mkdir();(output/'native').mkdir()
    (output/'source-manifest.json').write_bytes(source_path.read_bytes());write_json(output/'recipe.json',recipe)
    charts=[];reviews={};media={};source_inputs=None
    for row in recipe['charts']:
        ident=row['id']
        if not isinstance(ident,str) or not ident.replace('-','').replace('_','').isalnum() or ident in reviews:raise ValueError('Unsafe/duplicate chart id')
        saved,state,parent=capture(root,row['projection'],swf_hash,inputs)
        family=row['family']
        optional_review=None;optional_errors=[]
        if row.get('optionalColorReview'):
            b=row['optionalColorReview'];optional_review=json.loads(checked(root,b['path'],b['sha256']).read_text())
            for missing in optional_review['missingResources']:checked(root,missing['path'],missing['sha256'])
            state,optional_errors=review_optional_colors(state,optional_review)
        if family in ('dac-5','dac-fixed-5'):
            projected,iteration=dac_five_projection(state,fixed=family=='dac-fixed-5');clock=None
        elif family in ('child-timeline-5','dac-7'):
            projected,iteration=extract_movie_arrays(state,family,row.get('sourcePrefix',''));clock=None
        elif family in ('scheduled-counter','scheduled-constant','parafla-counter'):
            projected=constant_projection(state,row['mode'])if family=='scheduled-constant'else scheduled_projection(state,row['mode'],allow_missing_decorative_sprites=row.get('reviewMissingDecorativeSprites',False),parafla=family=='parafla-counter');clock=projected['clockField'];iteration=projected['preparedIteration']
        else:raise ValueError('Unknown native family')
        motion_path,motion_state,motion_parent=capture(root,row['motionCapture'],swf_hash,inputs)
        if optional_review:motion_state,_=review_optional_colors(motion_state,optional_review)
        motion_args=row['motion'];observed={**motion_state,'captures':[i for i in motion_state['captures']if i['frame']in motion_args['iterations']]}
        motion=movie_observation(observed,projected,motion_args['note'],motion_args['receptor'],motion_args['lane'],
                                 motion_args.get('targetY'),motion_args.get('geometryReference'),clock)
        audio_path,audio_state,audio_parent=capture(root,row['audioCapture'],swf_hash,inputs)
        if optional_review:audio_state,_=review_optional_colors(audio_state,optional_review)
        audit_ok(audio_state,projected.get('presentationDiagnostics',[]))
        origin=movie_audio_origin(audio_state,clock)
        # Bind the logical counter clock between observed music start and motion,
        # rather than borrowing a different chart's timing or source geometry.
        expected_offset=origin['logicalFrame']-origin['iteration']
        if clock:
            for sample in motion['samples']:
                if sample['movieFrame']-sample['iteration']!=expected_offset:raise ValueError('Audio and movement counters disagree')
        elif any(sample['movieFrame']-sample['iteration']!=expected_offset for sample in motion['samples']):
            raise ValueError('Audio and movement movie clocks disagree')
        config=audio_state['source']['config'];movie=(inputs/config['movie']).resolve(strict=True)
        if not movie.is_relative_to(inputs.resolve()) or digest(movie.read_bytes())!=swf_hash:raise ValueError('Movie input changed')
        route,mp3,audio=stream_path(movie,origin);mp3_hash=digest(mp3)
        if mp3_hash not in media:
            directory=output/'media'/mp3_hash;directory.mkdir();(directory/'source.mp3').write_bytes(mp3)
            audio_report=canonical_audio(directory/'source.mp3',audio_path.parent,origin,directory,ffmpeg)
            write_json(directory/'stream.json',audio)
            media[mp3_hash]={'path':str((directory/'music.wav').relative_to(output)),'sha256':audio_report['canonicalSha256'],'mimeType':'audio/wav'}
        else:
            # A shared media hash alone does not prove two difficulties' clocks.
            reference=next(c for c in charts if c['music']==media[mp3_hash])
            if reference['audioOrigin']['logicalFrame']!=origin['logicalFrame']:raise ValueError('Shared audio charts have different origins')
            comparison_directory=output/'references'/(ident+'-audio');comparison_directory.mkdir()
            comparison=canonical_audio(output/'media'/mp3_hash/'source.mp3',audio_path.parent,origin,comparison_directory,ffmpeg)
            if comparison['canonicalSha256']!=media[mp3_hash]['sha256']:raise ValueError('Shared audio charts have different native decoder origins')
        metadata={**recipe.get('metadata',{})}
        if not metadata.get('artist'):
            label=projected['metadata'].get('ARTIST_NAME','')
            if label.startswith('Artist')and(artist:=label[len('Artist'):].strip())not in ('','undefined','null'):metadata['artist']=artist
        if source.get('artist') and not metadata.get('artist'):metadata['artist']=source['artist']
        chart=relative_chart(projected,origin['logicalFrame']);chart.update(id=ident,music=media[mp3_hash],audioOrigin=origin,
            nativeReference=f'references/{ident}.json',effects=([{'type':'per-arrow-speed-approximation','nativeSpeedChanges':projected['nativeSpeedChanges'],'scope':'SSC global scroll approximates source per-arrow spawn speeds; note timestamps are preserved'}]if projected.get('motionSpeedPolicy')=='per-arrow-at-spawn'else[])+([{'type':'missing-optional-color','review':optional_review,'diagnostics':optional_errors}]if optional_review else[])+([{'type':'unmapped-native-presentation','data':projected['nativePresentation']}]if projected.get('nativePresentation',{}).get('iro_dac')else[])+([{'type':'inactive-native-lane-schedules','data':projected['inactiveSchedules']}]if any(projected.get('inactiveSchedules',{}).get(k)for k in ('taps','holds'))else[])+[{'type':'source-cursor-order','lane':index,**issue} for index,lane in enumerate(projected['lanes']) for issue in lane.get('sourceOrderIssues',[])],metadata={**metadata,'difficultyLabel':row.get('label',projected['metadata'].get('diffi',''))},difficultyStars=None)
        reference={'schema':'ddi-native-family-reference/experimental-1','family':family,'sourceSwfSha256':swf_hash,
                   'preparedIteration':iteration,'clock':clock or 'root movie frame','audioOrigin':origin,'projection':projected,
                   'motion':motion,'metadata':chart['metadata'],'sourceSnapshotSha256':digest(saved.read_bytes()),
                   'limits':['Source frame data preserved; original Flash/device latency not measured',
                             'Frame-phase differences up to one native tick remain observable in original rendering',
                             'Unsorted source arrays and native presentation diagnostics are retained; arbitrary engine customization is not reproduced']}
        write_json(output/chart['nativeReference'],reference)
        motion_relative=f'references/{ident}-motion.json';write_json(output/motion_relative,motion)
        reviews[ident]={'baseArrowHeightsPerSecond':motion['baseArrowHeightsPerSecond'],
                        'evidence':{'path':motion_relative,'sha256':digest((output/motion_relative).read_bytes())},
                        'scope':motion['scope']}
        for label,path in [('projection',saved),('motion',motion_path),('audio',audio_path)]:
            target=output/'native'/ident/label
            copy_tree(path.parent,target)
        source_inputs=inputs
        charts.append(chart)
    if not charts:raise ValueError('No initialized native charts')
    copy_tree(source_inputs,output/'source/inputs')
    manifest={'schema':'ddi-normalized-work/experimental-1','adapter':'native-families/experimental-1',
              'adapterSourceSha256':digest(Path(__file__).read_bytes()),'familySourceSha256':digest((Path(__file__).parent/'native_families.py').read_bytes()),
              'recipeSha256':digest(raw_recipe),'sourceManifestSha256':recipe['source']['sha256'],'sourceSwfSha256':swf_hash,
              'sourcePageUrl':source.get('pageUrl',source.get('page',{}).get('url')),
              'capturedAt':parent['startedAt'],'metadata':recipe.get('metadata',{}),'charts':charts,
              'assessment':{'nativeReference':'source/schedule and movement cross-checked','audio':'demuxed observed continuous slice; native PCM origin matched',
                            'conversion':'requires game-parser verification before promotion','sourceData':'local only'}}
    write_json(output/'manifest.json',manifest)
    # Review evidence remains in this immutable export, safe to bind/copy later.
    for ident,review in reviews.items():review['evidence']['path']=str(output/review['evidence']['path'])
    write_json(output/'native-scroll-review.json',{'schema':'ddi-reviewed-native-scroll/experimental-1',
               'sourceManifestSha256':digest((output/'manifest.json').read_bytes()),'charts':reviews})
    return manifest


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('recipe',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--ffmpeg',type=Path,default=shutil.which('ffmpeg'));args=parser.parse_args()
    result=extract(args.recipe,args.output,args.ffmpeg)
    print(json.dumps({'charts':len(result['charts']),'taps':sum(len(l['tapsSeconds'])for c in result['charts']for l in c['lanes']),
                      'holds':sum(len(l['holdsSeconds'])for c in result['charts']for l in c['lanes'])}))
