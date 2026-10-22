# NIGO engine RC.3 수신 — 2026-10-22

**수신·전체 identity·선정 JRE의 실제 `engine-info` PASS. RC.3 채택/lock 변경·제품 package/A7 인수는 수행하지 않았다.**
기존 RC.1/RC.2 lock·snapshot·제품 결과를 유지한다. NIGO source를 재빌드하거나 노드/DB/서비스를 실행하지 않았다.

## 직접 수신한 후보

| 항목 | 값 |
| --- | --- |
| NIGO 인계 문서 revision | `75c2666c2719ff39499f4772821f71d0a425d90e` |
| 공급 라인 / 태그 | `release/engine-0.1` / `nigo-engine-v0.1.0-rc.3` |
| 태그 object / source | `aef61783e7d3e34b83189264be2221d1d782ede3` / `7ec5abd8c98e1cf070763a16f815e9281b74a5c1`, dirty=false |
| JAR SHA-256 / bytes | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` / 164,088,530 |
| archive SHA-256 / bytes | `c080b0366a43b755868d359533712bb64122187d361497689501c7a5910c02a3` / 157,657,740 |
| 원본 manifest SHA-256 | `7aada98c54f21ca9ad326c4a0120455675fd9ad6ffe4cc7c0017d3578d67e407` |
| 계약 fingerprint | `c604b1d24097015b2839d97f2bc9d45e3fff91ed6984665d1aa827a155c7b6e8` |
| Console fingerprint | `a76e634dacf728b81cd446cdeaa2b6a250e1a2a91c505153dd424fd46431e99e` |
| runtime | Eclipse Adoptium Temurin21.0.12.1+1-LTS full JRE / macOS arm64, 259파일·49모듈 |
| Java launcher SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| runtime inventory 원문 SHA-256 | `fb779d849cdb9153b694b3ad368dc64b178af71a118d148dcd4d51a3e7853761` |
| 공급 채널 / 계약 | `development`, `officialRelease=false`, `PROPOSED` |

[고정 인계 문서](https://github.com/bwg-loonshots/nigo-protocol/blob/75c2666c2719ff39499f4772821f71d0a425d90e/requirements/REQ-0003-engine-rc3-handoff.md)와
[private prerelease](https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.3)를 직접 확인했다.
release ID `420224820`, archive/checksum asset ID `664355408`/`664355411`, 게시 시각 `2026-10-22T01:46:24Z`다.
GitHub 읽기 API의 digest·size·tag peeled source와 받은 bytes를 대조했다. tag는 `unsigned`, release는
`immutable=false`, release branch는 `protected=false`다. checksum 대조를 공급자 서명 검증이나 강제 불변 보호로 표시하지 않는다.

## 검증 범위와 원문 보존

- 별도 새 경로에 archive/checksum을 직접 다운로드하고 trusted hash/크기·sidecar·gzip을 검사했다.
- 절대/비정규 경로·중복·link/special file 없이 고정 digest 디렉터리의 일반 파일22개만 추출했다.
  `SHA256SUMS` 외21개 전체 membership과 checksum이 일치했다.
- 원본 manifest·JAR의 build/Console JSON·공급 engine-info 전체 identity가 일치했다. 계약4개의
  source/archive/JAR 원문과 fingerprint, 운영 문서3개의 고정 source도 대조했다.
- 고정 source tree에서 Console source fingerprint를 재계산하고 JAR의20개 Console 파일을 공급 inventory와 비교했다.
- 공급 요약을 재집계해 source1,280 PASS/기존 opt-in2 NOT_RUN, canonical packaged CLI9/Storage8,
  외부 선정 JRE CLI9/Storage10을 구분했다. Storage118 child/100명령과3복구경로의 정제 요약을 대조했다.
  이는 공급자 원자료 독립 감사나 BXDL의 해당 테스트 실행이 아니다.
- 같은 선정 full JRE의259파일 membership/hash/size/mode를 실행 전후 대조했고 실제 module49개와 runtime properties를 확인했다.
  받은 JAR의 `engine-info`는 exit0·stderr0·단일 JSON이며 공급본/manifest/JAR의 전체 identity와 일치했다.
  실행 후 공급22파일 membership/hash/size/mode도 동일했다.

[consumer-receipt.json](./consumer-receipt.json), [실제 engine-info](./consumer-evidence/engine-info.json),
[실행 범위 기록](./consumer-evidence/engine-info-verification.json)이 BXDL 수신 근거다.
Java의 빈 property를 읽는 로컬 검증기에서 발생한 도구 오류는 engine-info 실행 전에 정정했고 최초 검증기/오류 이력을 별도 보존했다.
엔진 후보 실패나 노드 시험 재시도로 분류하지 않는다.

이 snapshot의 공급 원문20개는 bytes 그대로다. `supplier-README.md`와 `supplier-SHA256SUMS`는 원래 이름만
분리했으며 원문 상대 링크는 NIGO/공급 bundle 기준이다. supplier checksum은 원래 JAR/checksum을 포함한
21개 inventory이므로 이 평탄화 snapshot에 직접 적용하지 않는다. 이 폴더의 `SHA256SUMS`는 자신을 제외한
24개 파일을 확인하는 별도 BXDL inventory다. JAR·archive·JRE·개인키·DB·미정제 로그는 Git에 넣지 않는다.

원본 manifest의 `NOT_RECORDED`/`NOT_RUN`/`NOT_CHECKED`/`PROPOSED`와 supplier evidence의 게시 전 상태,
과거 실패 이력은 보존한다. 새 수신 판정은 위 별도 소비자 receipt에서만 기록한다.

## 다음 선택과 미검증 범위

RC.2 대비 공식 `storage-integrity`와 Console 패치가 포함되어 계약/Console fingerprint도 바뀌었다.
새 후보를 제품에 채택하려면 별도 명시 lock/package spec과 호환성 검토·제품 인수가 필요하다.
RC.2 지정 package PASS나 공급자의 Storage/QBFT PASS를 RC.3 제품/A7 결과로 자동 승계하지 않는다.

이번 수신은 `init`/`run`/`storage-integrity`/서비스 동작을 실행하지 않았다. 고객 DB·기존 설치·키·lock을
변경하지 않았다. A7의 서비스 중지·자동 재기동 차단·backend/경로 매핑·백업·명시 repair 확인·UNKNOWN 처리와
제품 재시작은 별도 구현/인수다. 전체 G1-M/J4·J5·기존 설치 upgrade/downgrade·Linux/systemd·Docker·multi-host·
장기/성능·Native kit 전체 인수도 별도다. NIGO 자체 배포 조건 `UNDECIDED`, 고객 배포 `NOT_GRANTED`와
과거 비용 실패 직접 원인 `UNRESOLVED`를 유지하며 REQ-0002/0003 전체 VERIFIED를 선언하지 않는다.
